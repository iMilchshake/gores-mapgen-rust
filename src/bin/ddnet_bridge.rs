use clap::{crate_version, Parser};
use core::net::{IpAddr, Ipv4Addr, SocketAddr};
use gores_mapgen_rust::config::MapConfig;
use gores_mapgen_rust::random::Seed;
use gores_mapgen_rust::{config::GenerationConfig, generator::Generator};
use itertools::Itertools;
use log::{debug, error, info, warn};
use simple_logger::SimpleLogger;
use std::collections::HashMap;
use std::panic;

use regex::Regex;
use std::{path::PathBuf, process::exit, str::FromStr, time::Duration};
use telnet::{Event, Telnet};

#[derive(Parser, Debug)]
#[command(name = "DDNet Bridge")]
#[command(version = crate_version!())]
#[command(about = "Detect DDNet-Server votes via econ to trigger map generations", long_about = None)]
enum Command {
    #[clap(name = "start", about = "start the ddnet bridge")]
    StartBridge(BridgeArgs),

    #[clap(
        name = "list",
        about = "print a list of available map- & generation configs"
    )]
    ListConfigs,
}

#[derive(Parser, Debug)]
struct BridgeArgs {
    /// ec_password
    econ_pass: String,

    /// ec_port
    econ_port: u16,

    /// telnet buffer size (amount of bytes/chars)
    #[arg(default_value_t = 256, long, short('b'))]
    telnet_buffer: usize,

    /// debug to console
    #[arg(short, long, default_value_t = false)]
    debug: bool,

    /// path to maps folder
    maps: PathBuf,

    /// how many times generation is retried
    #[arg(default_value_t = 10, long, short('r'))]
    generation_retries: usize,
}

#[derive(Debug)]
struct Vote {
    _player_name: String,
    vote_name: String,
    vote_reason: String,
}

struct Econ {
    telnet: Telnet,
    authed: bool,
}

impl Econ {
    pub fn new(port: u16, buffer_size: usize) -> Econ {
        let address = SocketAddr::new(
            IpAddr::V4(Ipv4Addr::from_str("127.0.0.1").expect("Invalid address")),
            port,
        );

        Econ {
            telnet: Telnet::connect_timeout(&address, buffer_size, Duration::from_secs(10))
                .unwrap_or_else(|err| {
                    error!("Coulnt establish telnet connection\nError: {:?}", err);
                    exit(1);
                }),
            authed: false,
        }
    }

    pub fn read(&mut self) -> Option<String> {
        let event = self.telnet.read().expect("telnet read error");

        if let Event::Data(buffer) = event {
            Some(String::from_utf8_lossy(&buffer).replace('\0', ""))
        } else {
            None
        }
    }

    pub fn send_rcon_cmd(&mut self, mut command: String) {
        command.push('\n');
        self.telnet
            .write(command.as_bytes())
            .expect("telnet write error");
    }

    pub fn rcon_say(&mut self, message: String) {
        self.send_rcon_cmd(format!("say {message}"));
    }
}

/// keeps track of the server bridge state
struct ServerBridge {
    /// econ connection to game server
    econ: Econ,

    /// stores information about vote while its still pending
    pending_vote: Option<Vote>,

    /// stores all available generation configs
    gen_configs: HashMap<String, GenerationConfig>,

    /// stores all available map configs
    map_configs: HashMap<String, MapConfig>,

    /// selected map config
    current_map_config: MapConfig,

    /// stores start arguments
    args: BridgeArgs,
}

impl ServerBridge {
    fn new(args: BridgeArgs) -> ServerBridge {
        ServerBridge {
            econ: Econ::new(args.econ_port, args.telnet_buffer),
            pending_vote: None,
            gen_configs: GenerationConfig::get_all_configs(),
            map_configs: MapConfig::get_all_configs(),
            current_map_config: MapConfig::get_initial_config(),
            args,
        }
    }

    fn start(&mut self) {
        loop {
            if let Some(data) = self.econ.read() {
                debug!("[RECV DEBUG]: {:?}", data);

                if !self.econ.authed {
                    self.check_auth(&data);
                } else {
                    self.check_vote(&data);
                }
            }
        }
    }

    /// checks whether the econ message regards votes
    pub fn check_vote(&mut self, data: &String) {
        // this regex detects all possible chat messages involving votes
        let vote_regex = Regex::new(r"(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}) I chat: \*\*\* (Vote passed.*|Vote failed|'(.+?)' called .+ option '(.+?)' \((.+?)\))\n").unwrap();
        let result = vote_regex.captures_iter(&data);

        for mat in result {
            let _date = mat.get(1).unwrap();
            let message = mat.get(2);

            // determine vote event type
            if let Some(message) = message.map(|v| v.as_str()) {
                match message {
                    _ if message.starts_with("Vote passed") => {
                        info!("[VOTE]: Success");
                        self.handle_pending_vote();
                    }
                    "Vote failed" => {
                        self.pending_vote = None;
                        info!("[VOTE]: Failed");
                    }
                    // vote started messages begin with 'player_name'
                    _ if message.starts_with('\'') => {
                        let player_name = mat.get(3).unwrap().as_str().to_string();
                        let vote_name = mat.get(4).unwrap().as_str().to_string();
                        let vote_reason = mat.get(5).unwrap().as_str().to_string();

                        info!(
                            "[VOTE]: vote_name={}, vote_reason={}, player={}",
                            &vote_name, &vote_reason, &player_name
                        );

                        self.pending_vote = Some(Vote {
                            _player_name: player_name,
                            vote_name,
                            vote_reason,
                        });
                    }
                    // panic if for some holy reason something else matched the regex
                    _ => panic!(),
                }
            }
        }
    }

    /// checks whether the econ message regards authentication
    pub fn check_auth(&mut self, data: &String) {
        if data == "Enter password:\n" {
            self.econ.send_rcon_cmd(self.args.econ_pass.clone());
            info!("[AUTH] Sending login");
        } else if data.starts_with("Authentication successful") {
            info!("[AUTH] Success");
            info!("[GEN] Generating initial map");
            self.econ.authed = true;
            self.generate_and_change_map(
                &Seed::from_u64(1337),
                &GenerationConfig::get_initial_config(false),
                self.args.generation_retries,
            );
        } else if data.starts_with("Wrong password") {
            info!("[AUTH] Wrong Password!");
            std::process::exit(1);
        }
    }

    pub fn handle_pending_vote(&mut self) {
        if let Some(vote) = self.pending_vote.take() {
            if vote.vote_name.starts_with("generate") {
                // derive Seed from vote reason
                let seed = if vote.vote_reason == "No reason given" {
                    Seed::random()
                } else if let Ok(seed_u64) = vote.vote_reason.parse::<u64>() {
                    Seed::from_u64(seed_u64)
                } else {
                    Seed::from_string(&vote.vote_reason)
                };

                // split vote name to get selected preset
                let config_name = vote
                    .vote_name
                    .splitn(2, char::is_whitespace)
                    .nth(1)
                    .unwrap();

                // get config based on name
                let gen_config = self
                    .gen_configs
                    .get(config_name)
                    .expect("config does not exist!")
                    .clone();

                self.generate_and_change_map(&seed, &gen_config, self.args.generation_retries);
            } else if vote.vote_name.starts_with("change_layout") {
                // split vote name to get selected preset
                let config_name = vote
                    .vote_name
                    .splitn(2, char::is_whitespace)
                    .nth(1)
                    .unwrap();

                // get config based on name
                let map_config = self
                    .map_configs
                    .get(config_name)
                    .expect("config does not exist!")
                    .clone();

                info!("[GEN] changed layout to {config_name}");
                self.econ
                    .rcon_say(format!("[GEN] changed layout to {config_name}"));

                // overwrite current map config
                self.current_map_config = map_config;
            }
        } else {
            warn!("[VOTE] Vote Success, but no pending vote! unhandled vote type?");
            self.econ.rcon_say(
                "[VOTE] Vote Success, but no pending vote! unhandled vote type?".to_string(),
            );
        }
    }

    fn generate_and_change_map(
        &mut self,
        seed: &Seed,
        gen_config: &GenerationConfig,
        retries: usize,
    ) {
        let map_path = self
            .args
            .maps
            .canonicalize()
            .unwrap()
            .join("random_map.map");

        let info_txt = format!(
            "[GEN] Generating | seed={:?} | gen_cfg={:?} | map_cfg={:?}",
            &seed, &gen_config.name, &self.current_map_config.name
        );
        info!("{info_txt}");
        self.econ.rcon_say(info_txt);

        let gen_status = panic::catch_unwind(|| {
            Generator::generate_map(100_000, &seed, gen_config, &self.current_map_config)
        });

        match gen_status {
            // map was generated successfully
            Ok(Ok(map)) => {
                info!("[GEN] Finished Map Generation!");
                map.export(&map_path);
                info!("[GEN] Map was exported");
                self.econ.send_rcon_cmd("change_map random_map".to_string());
                self.econ.send_rcon_cmd("reload".to_string());
                self.econ.send_rcon_cmd("say [GEN] Done...".to_string());
            }
            // map generation failed -> just retry
            Ok(Err(generation_error)) => {
                warn!("[GEN] Generation Error: {:?}", generation_error);
                self.econ
                    .send_rcon_cmd(format!("say [GEN] Failed due to: {:}", generation_error));

                if retries > 0 {
                    // retry with different seed
                    let mut seed = seed.clone();
                    seed.seed_str = String::new();
                    seed.seed_u64 = seed.seed_u64.wrapping_add(1);
                    self.generate_and_change_map(&seed, gen_config, retries - 1);
                }
            }
            // map generation panic -> STOP
            Err(panic_info) => {
                error!("[ERROR] Generation panicked!");
                error!("{:?}", panic_info);
                self.econ
                    .rcon_say("[GEN] GENERATION PANICKED, THIS SHOULD NOT HAPPEN".to_string());
                self.econ
                    .rcon_say("[GEN] please report this to iMilchshake, thanks :D".to_string());
            }
        }
    }
}

fn print_configs() {
    println!(
        "GenerationConfig: {}",
        GenerationConfig::get_all_configs()
            .keys()
            .into_iter()
            .join(",")
    );
    println!(
        "MapConfig: {}",
        MapConfig::get_all_configs().keys().into_iter().join(",")
    );
}

fn main() {
    match Command::parse() {
        Command::StartBridge(bridge_args) => {
            SimpleLogger::new().init().unwrap();
            let mut bridge = ServerBridge::new(bridge_args);
            bridge.start();
        }
        Command::ListConfigs => print_configs(),
    }
}
