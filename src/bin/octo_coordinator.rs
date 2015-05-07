extern crate daemon;
extern crate iron;

use daemon::State;
use daemon::Daemon;
use daemon::DaemonRunner;
use iron::prelude::*;
use iron::status;
use std::env;
use std::fs::OpenOptions;
use std::io::Error;
use std::io::Write;
use std::sync::mpsc::Receiver;

fn hello_world(_: &mut Request) -> IronResult<Response> {
	Ok(Response::with((status::Ok, "Hello World!")))
}

fn main() {
	log("Example started.");
	let daemon = Daemon {
		name: "octobuild_coordinator".to_string()
	};
	daemon.run(move |rx: Receiver<State>| {
		log("Worker started.");
		let mut web = None;
		for signal in rx.iter() {
			match signal {
				State::Start => {
					log("Worker: Starting on 3000");
					web = Some(Iron::new(hello_world).http("localhost:3000").unwrap());
					log("Worker: Started");
				},
				State::Reload => {
					log("Worker: Reload");
				}
				State::Stop => {
					log("Worker: Stoping");
					match web.take() {
						Some(mut v) => {v.close().unwrap();}
						None => {}
					}
					log("Worker: Stoped");
				}
			};
		}
		log("Worker finished.");
	}).unwrap();
	log("Example finished.");
}


#[allow(unused_must_use)]
fn log(message: &str) {
	log_safe(message);
}

fn log_safe(message: &str) -> Result<(), Error> {
	println! ("{}", message);
	let path = try! (env::current_exe()).with_extension("log");
	let mut file = try! (OpenOptions::new().create(true).write(true).append(true).open(&path));
	try! (file.write(message.as_bytes()));
	try! (file.write(b"\n"));
	Ok(())
}
