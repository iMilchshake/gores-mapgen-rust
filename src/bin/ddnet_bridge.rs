use clap::Parser;
use regex::Regex;
use std::{process::exit, thread::sleep, time::Duration};
use telnet::{Event, Telnet};

#[derive(Parser, Debug)]
#[command(name = "DDNet Bridge")]
#[command(version = "0.1a")]
#[command(about = "Detect DDNet-Server votes via econ to trigger map generations", long_about = None)]
struct Args {
    /// ec_password
    econ_pass: String,

    /// ec_port
    econ_port: u16,

    /// telnet buffer size (amount of bytes/chars)
    #[arg(default_value_t = 256, long, short('b'))]
    telnet_buffer: usize,

    /// interval (in seconds) in which telnet messages are received in buffer
    #[arg(default_value_t = 0.5, long, short('i'))]
    telnet_interval: f32,

    /// debug to console
    #[arg(short, long)]
    debug: bool,
}

#[derive(Debug)]
struct Vote {
    player_name: String,
    vote_name: String,
    vote_reason: String,
}

struct Econ {
    telnet: Telnet,
}

impl Econ {
    pub fn new(port: u16, buffer_size: usize) -> Econ {
        Econ {
            telnet: Telnet::connect(("localhost", port), buffer_size).unwrap_or_else(|err| {
                println!("Coulnt establish connection\nError: {:?}", err);
                exit(1);
            }),
        }
    }

    pub fn read(&mut self) -> Option<String> {
        let event = self.telnet.read_nonblocking().expect("telnet read error");

        if let Event::Data(buffer) = event {
            Some(String::from_utf8_lossy(&buffer).replace("\0", ""))
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

    pub fn handle_vote(&mut self, vote: &Vote) {
        if vote.vote_name == "generate" {
            println!("[DEBUG] Generating Map...");

            // TODO: Actually generate map here ...

            self.send_rcon_cmd("say [DEBUG] Generating Map...".to_string());
        }
    }
}

fn main() {
    let args = Args::parse();

    if args.debug {
        dbg!(&args);
    }

    // this regex detects all possible chat messages involving votes
    let vote_regex = Regex::new(r"(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}) I chat: \*\*\* (Vote passed|Vote failed|'(.+?)' called .+ option '(.+?)' \((.+?)\))\n").unwrap();
    let mut econ = Econ::new(args.econ_port, args.telnet_buffer);
    let mut pending_vote: Option<Vote> = None;

    loop {
        if let Some(data) = econ.read() {
            if args.debug {
                println!("[RECV DEBUG]: {:?}", data);
            }

            if data == "Enter password:\n" {
                econ.send_rcon_cmd(args.econ_pass.clone());
                println!("[AUTH] Sending login");
            } else if data.starts_with("Authentication successful") {
                println!("[AUTH] Success");
            } else if data.starts_with("Wrong password") {
                println!("[AUTH] Wrong Password!");
                std::process::exit(1);
            } else {
                let result = vote_regex.captures_iter(&data);

                for mat in result {
                    let _date = mat.get(1).unwrap();
                    let message = mat.get(2);

                    // determine vote event type
                    if let Some(message) = message.map(|v| v.as_str()) {
                        match message {
                            "Vote passed" => {
                                println!("[VOTE]: Success");
                                econ.handle_vote(pending_vote.as_ref().unwrap());
                            }
                            "Vote failed" => {
                                pending_vote = None;
                                println!("[VOTE]: Failed");
                            }
                            // vote started messages begin with 'player_name'
                            _ if message.starts_with("'") => {
                                let player_name = mat.get(3).unwrap().as_str().to_string();
                                let vote_name = mat.get(4).unwrap().as_str().to_string();
                                let vote_reason = mat.get(5).unwrap().as_str().to_string();

                                println!(
                                    "[VOTE]: vote_name={}, vote_reason={}, player={}",
                                    &vote_name, &vote_reason, &player_name
                                );

                                pending_vote = Some(Vote {
                                    player_name,
                                    vote_name,
                                    vote_reason,
                                });
                            }
                            _ => panic!(),
                        }
                    }
                }
            }
        }

        sleep(Duration::from_secs_f32(args.telnet_interval));
    }
}