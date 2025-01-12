use std::{
    fs::File,
    io::{self, BufRead},
};

use dirs::home_dir;

use x11rb::protocol::xproto::{KeyButMask, ModMask};
use xkbcommon::xkb::{self, Keysym};

pub struct Bind {
    pub key: Keysym,
    pub mods: ModMask,
    pub keybutmask: KeyButMask,
    pub cmd: Option<Vec<String>>,
}

pub struct Config {
    pub binds: Vec<Bind>,
}

pub fn plain_into_bind(data: &str) -> Option<Bind> {
    let parts: Vec<&str> = data.split('=').collect();
    if parts.len() == 2 {
        let keys: Vec<&str> = parts[0].split('+').collect();

        let mut temp_but_mask = KeyButMask::default();
        let mut temp_mods = ModMask::default();
        let mut temp_key = Keysym::default();
        for key in keys {
            match key.to_ascii_lowercase().as_str() {
                "shift" => {
                    temp_mods |= ModMask::SHIFT;
                    temp_but_mask |= KeyButMask::SHIFT;
                }
                "control" | "ctrl" => {
                    temp_mods |= ModMask::CONTROL;
                    temp_but_mask |= KeyButMask::CONTROL;
                }
                "alt" | "mod1" => {
                    temp_mods |= ModMask::M1;
                    temp_but_mask |= KeyButMask::MOD1;
                }
                "numlock" | "mod2" => {
                    temp_mods |= ModMask::M2;
                    temp_but_mask |= KeyButMask::MOD2;
                }
                "mod3" => {
                    temp_mods |= ModMask::M3;
                    temp_but_mask |= KeyButMask::MOD3;
                }
                "super" | "mod4" | "tux" => {
                    temp_mods |= ModMask::M4;
                    temp_but_mask |= KeyButMask::MOD4;
                }
                "mod5" => {
                    temp_mods |= ModMask::M5;
                    temp_but_mask |= KeyButMask::MOD5;
                }
                key => {
                    let temp = xkb::keysym_from_name(key, 0);
                    if temp.eq(&Keysym::NoSymbol) {
                        continue;
                    }
                    println!("{temp:?}");
                    temp_key = temp;
                }
            }
        }

        let cmd = shlex::split(parts[1]);
        Some(Bind {
            key: temp_key,
            keybutmask: temp_but_mask,
            mods: temp_mods,
            cmd,
        })
    } else {
        None
    }
}

impl Config {
    pub fn load_plain() -> Self {
        let path = home_dir()
            .expect("Failed to get $HOME dir")
            .join(".config/seppun/kb");

        let config_file = File::open(path).expect("Failed to open file");

        let reader = io::BufReader::new(config_file);
        let mut binds: Vec<Bind> = vec![];

        for line in reader.lines() {
            if let Ok(buffer) = line {
                if buffer.starts_with("///") {
                    continue;
                }
                match plain_into_bind(&buffer) {
                    Some(bind) => binds.push(bind),
                    None => {
                        eprintln!("Invalid bind structure");
                        continue;
                    }
                }
            }
        }
        Config { binds }
    }
}
