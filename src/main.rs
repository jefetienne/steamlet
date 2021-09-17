use structopt::StructOpt;
use std::process::Command;
use std::path::PathBuf;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::{BufReader, BufWriter, SeekFrom};
use std::io::prelude::*;
use std::collections::HashMap;

extern crate dirs;
//https://github.com/seanmonstar/reqwest
//https://crates.io/crates/curl

#[derive(StructOpt, Debug)]
enum SteamletCommand {
	/// Plays a Steam game via an alias or by a Steam game ID (with -i)
	Play {
		/// Flag to use a game ID instead of an alias
		#[structopt(short = "i", long = "id")]
		use_id: bool,

		/// The input for selecting the game (an alias or an ID with the '-i' flag)
		#[structopt(name = "game")]
		game_str: String,
	},

	/// Adds or sets an alias to an associated Steam game ID (alt. command 'add')
	#[structopt(alias = "add")]
	Set {
		/// The alias to be made
		alias: String,

		/// The Steam game ID to be associated with
		#[structopt(name = "steam_id")]
		id: u32
	},

	/// Removes an alias (alt. command 'rm')
	#[structopt(alias = "rm")]
	Remove {
		/// List of one or more aliases to be removed
		#[structopt(required = true, min_values = 1)]
		aliases: Vec<String>
	},

	/// Lists all aliases and their associated Steam game IDs
	List
}

/// Run Steam games on the commandline intuitively via aliases or IDs
#[derive(StructOpt, Debug)]
#[structopt(
	name = "steamlet",
	after_help = r#"EXAMPLES:
	Play a Steam game using the Steam game ID:
		steamlet play -i 227300

	Add an alias with an associated ID:
		steamlet add ets2 227300

	Play a Steam game with an alias:
		steamlet play ets2

	You can also use spaces in your aliases with double-quotes:
		steamlet add "euro truck simulator 2" 227300

	Remove alias(es):
		steamlet remove ets2 "euro truck simulator 2" [...]

	List saved aliases:
		steamlet list
"#
)]
struct Steamlet {
	#[structopt(subcommand)]
	command: SteamletCommand
}

static DATA_FILE_NAME: &'static str = "steamlet.json";

fn run_steam_game(game_id: u32) {
	println!("-------------------------------------------------");

	// Search to see if the steam flatpak exists
	let mut flatpak = Command::new("flatpak")
		.arg("list")
		.stdout(std::process::Stdio::piped())
		.spawn()
		.expect("'flatpak' command failed to start");

	let mut grep = Command::new("grep")
		.arg("com.valvesoftware.Steam")
		.stdin(std::process::Stdio::piped())
		.stdout(std::process::Stdio::piped())
		.spawn()
		.expect("'grep' command failed to start");
	
	if let Some(ref mut stdout) = flatpak.stdout {
		if let Some(ref mut stdin) = grep.stdin {
			let mut buf: Vec<u8> = Vec::new();
			stdout.read_to_end(&mut buf).unwrap();
			stdin.write_all(&buf).unwrap();
		}
	}

	let res = grep.wait_with_output().unwrap().stdout;

	match String::from_utf8(res) {
		Ok(v) => {
			// If Steam flatpak exists, run that
			if v.contains("Steam") {
				Command::new("flatpak")
					.arg("run")
					.arg("com.valvesoftware.Steam")
					.arg(format!("steam://run/{}", game_id))
					.spawn()
					.expect("'flatpak run com.valvesoftware.Steam' command failed to start");
			} else {
				// Otherwise, try to run the direct steam command
				Command::new("steam")
				.arg(format!("steam://run/{}", game_id))
				.spawn()
				.expect("'steam' command failed to start");
			}
		}
		Err(e) => panic!("Invalid UTF-8 sequence: {}", e),
	};
}

fn get_alias_data() -> (File, HashMap<String, u32>) {
	// Get local data directory
	let data_dir: PathBuf = dirs::data_local_dir().unwrap().join("steamlet");
	let data: HashMap<String, u32>;
	let file: File;

	// Create a new file if the local data directory does not exist
	if !data_dir.exists() {
		std::fs::create_dir_all(data_dir.as_path()).unwrap();

		file = OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(data_dir.join(DATA_FILE_NAME).as_path())
			.unwrap();

		data = HashMap::new();
	} else {
		file = OpenOptions::new()
			.read(true)
			.write(true)
			.open(data_dir.join(DATA_FILE_NAME).as_path())
			.unwrap();

		let buf_reader = BufReader::new(&file);
		// Read file contents into HashMap
		data = serde_json::from_reader(buf_reader)
			.unwrap_or(HashMap::new());

		//println!("Found data file '{}'", data_dir.to_str().unwrap());
	}

	(file, data)
}

fn write_to_data_file(file: File, data: HashMap<String, u32>, message: String) {
	// Create BufWriter for the file
	let mut buf_writer = BufWriter::new(&file);

	// Clear the file contents and set the cursor to position '0'
	file.set_len(0).unwrap();
	buf_writer.seek(SeekFrom::Start(0)).unwrap();

	// Write data to the file
	match serde_json::to_writer_pretty(buf_writer, &data) {
		Ok(_) => {
			println!("{}", message);
			// TODO: How to flush?
			//buf_writer.flush().unwrap();
		},
		Err(_) => {
			println!("Error while writing to {}", DATA_FILE_NAME);
		}
	}
}

fn main() {
	let args = Steamlet::from_args();

	//println!("{:?}\n\n-----------", args);
	match args.command {
		SteamletCommand::Play { use_id, game_str } => {
			if use_id {
				// Play steam game via the id itself
				match game_str.parse::<u32>() {
					Ok(id) => {
						println!("Starting application with ID '{}'", id);
						run_steam_game(id);
					}
					Err(_) => println!("Steam ID must be a number")
				}
			} else {
				// Play steam game via the player-made alias
				let data: HashMap<String, u32> = get_alias_data().1;
				let game = &game_str.to_lowercase();

				match data.get(game) {
					Some(id) => { 
						println!("Starting {} ({})", game, *id);
						run_steam_game(*id);
					}
					None => println!("Could not find alias '{}'", game)
				}
			}
		},
		SteamletCommand::Set { alias, id } => {
			// Get the file and parsed data
			let tuple = get_alias_data();
			let file: File = tuple.0;
			let mut data: HashMap<String, u32> = tuple.1;

			// Create/update the alias with the associated steam_id
			let formatted: String = alias.trim().to_lowercase();

			if formatted.len() > 0 {
				data.insert(formatted.to_string(), id);

				let message = format!("Alias '{}' successfully set to {}; total aliases = {}", &formatted, id, data.len());

				write_to_data_file(file, data, message);
			} else {
				println!("Alias must not be empty");
			}
		},
		SteamletCommand::Remove { mut aliases } => {
			// Get the file and parsed data
			let tuple = get_alias_data();
			let file: File = tuple.0;
			let mut data: HashMap<String, u32> = tuple.1;

			// Filter out the list of aliases that don't exist in 'data'
			// We use the 'aliases' list to print out what did get successfully
			// removed
			aliases.retain(|a| {
				let b = data.contains_key(a);

				if !b {
					println!("Alias '{}' not found", a);
				}

				return b;
			});

			// If there are existing aliases, remove them
			if aliases.len() > 0 {
				// Filter out the entries in 'data' whose key exists in 'aliases'
				data.retain(|key, _| {
					return !aliases.contains(key);
				});

				let mut list: String = String::new();
				let mut first = true;

				for item in aliases {
					if !first {
						list += ", ";
					}
					list += &item;
					first = false;
				}

				let message = format!("Aliases '{}' successfully removed; total aliases = {}", list, data.len());

				write_to_data_file(file, data, message);
			} else {
				println!("Nothing to be removed; total aliases = {}", data.len());
			}
		},
		SteamletCommand::List => {
			// Get the file and parsed data
			let tuple = get_alias_data();
			let data: HashMap<String, u32> = tuple.1;
			let tab_size = 4.0;
			let num_tabs: usize = 4;

			println!("Path: {}\n", dirs::data_local_dir().unwrap().join("steamlet").join(DATA_FILE_NAME).to_str().unwrap());

			// Sort results alphabetically
			let mut sorted: Vec<_> = data.into_iter().collect();
			sorted.sort_by(|x,y| x.0.cmp(&y.0));

			for kv in &sorted {
				let calc = ((kv.0.len() as f64) / tab_size).round() as usize;
				let spaces: String = std::iter::repeat("\t").take(num_tabs).collect::<String>();

				// If the alias is longer than the default of 'num_tabs' tabs, put the id on a separate line
				if calc > num_tabs {
					println!("{}", kv.0);
					println!("{}{}", spaces, kv.1);
				} else {
					println!("{}{}{}", kv.0, spaces, kv.1);
				}
			}
		},
	}
}


/*
#[cfg(test)]
mod tests {
	//https://github.com/TeXitoi/structopt/blob/master/examples/required_if.rs
	use super::*;

	#[test]
	fn test_play_no_id_flag() {
		let opt = Steamlet::from_iter_safe(&["play", "227300"]);
		let err = opt.unwrap_err();
		assert_eq!(err.kind, clap::ErrorKind::MissingRequiredArgument);
	}
}



*/