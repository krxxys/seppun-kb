use clap::{command, Args, Parser, Subcommand};
use config::{plain_into_bind, Config};
use daemonize::Daemonize;
use dirs::home_dir;
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use signal_hook::consts::{SIGHUP, SIGTERM};
use signal_hook::iterator::Signals;
use signal_hook::low_level::exit;
use std::fs::OpenOptions;
use std::io::Write;
use std::process::Command;
use std::sync::Arc;
use std::{fs, thread};
use std::{fs::File, rc::Rc, sync::atomic::AtomicBool};
use x11rb::{
    connection::Connection,
    protocol::{
        xkb::ConnectionExt,
        xproto::{self, *},
        Event,
    },
    reexports::x11rb_protocol::protocol::xkb,
    xcb_ffi::XCBConnection,
};
use xkbcommon::xkb::{self as xkbc, Context, Keycode, Keymap, State};
mod config;

#[allow(dead_code)] //hold ctx maybe project need it in future
struct AppXkb {
    ctx: Context,
    keymap: Keymap,
    device_id: i32,
    state: State,
}
struct App {
    connection: Rc<XCBConnection>,
    xkb: AppXkb,
    pub config: Config,
    root_window: u32,
    flags: Arc<AppFlags>,
}

pub struct AppFlags {
    kill: Arc<AtomicBool>,
    reload: Arc<AtomicBool>,
}

#[derive(Parser)]
#[command(author, version, about)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Args)]
pub struct AddArgs {
    data: String,
}

#[derive(Subcommand)]
pub enum Commands {
    Start,
    Stop,
    Reload,
    Add(AddArgs),
}

fn init_app() -> Result<App, Box<dyn std::error::Error>> {
    let (connection, screen_number) =
        XCBConnection::connect(None).expect("Failed to connect to xcb server");
    let connection_rc = Rc::new(connection);
    connection_rc
        .xkb_use_extension(1, 0)?
        .reply()
        .expect("Failed to use xkb extension");
    let events = xkb::EventType::NEW_KEYBOARD_NOTIFY
        | xkb::EventType::MAP_NOTIFY
        | xkb::EventType::STATE_NOTIFY;
    let map_parts = xkb::MapPart::KEY_TYPES
        | xkb::MapPart::KEY_SYMS
        | xkb::MapPart::MODIFIER_MAP
        | xkb::MapPart::EXPLICIT_COMPONENTS
        | xkb::MapPart::KEY_ACTIONS
        | xkb::MapPart::KEY_ACTIONS
        | xkb::MapPart::KEY_BEHAVIORS
        | xkb::MapPart::VIRTUAL_MODS
        | xkb::MapPart::VIRTUAL_MOD_MAP;

    connection_rc.xkb_select_events(
        xkb::ID::USE_CORE_KBD.into(),
        0u8.into(),
        events,
        map_parts,
        map_parts,
        &xkb::SelectEventsAux::new(),
    )?;

    let ctx = xkbc::Context::new(xkbc::CONTEXT_NO_FLAGS);
    let dev_id = xkbc::x11::get_core_keyboard_device_id(&connection_rc);
    let keymap = xkbc::x11::keymap_new_from_device(
        &ctx,
        &connection_rc,
        dev_id,
        xkbc::KEYMAP_COMPILE_NO_FLAGS,
    );

    let state = xkbc::x11::state_new_from_device(&keymap, &connection_rc, dev_id);

    let root_window = &connection_rc.setup().roots[screen_number].root;
    let values =
        ChangeWindowAttributesAux::new().event_mask(EventMask::KEY_PRESS | EventMask::KEY_RELEASE);
    xproto::change_window_attributes(&connection_rc, root_window.to_owned(), &values)
        .expect("Failed to change root window atributes");
    let config = Config::load_plain();

    let reload_flag = Arc::new(AtomicBool::new(false));
    let kill_flag = Arc::new(AtomicBool::new(false));
    let app_flags = Arc::new(AppFlags {
        reload: reload_flag,
        kill: kill_flag,
    });

    Ok(App {
        connection: connection_rc.clone(),
        xkb: AppXkb {
            ctx,
            keymap,
            device_id: dev_id,
            state,
        },
        config,
        root_window: *root_window,
        flags: app_flags,
    })
}

fn handle_app(app: &mut App) {
    loop {
        /* flags check */
        if app.flags.reload.load(std::sync::atomic::Ordering::SeqCst) {
            app.flags
                .reload
                .store(false, std::sync::atomic::Ordering::SeqCst);
            ungrab_all_binds(app);
            let new_config = Config::load_plain();
            app.config = new_config;
            grab_all_binds(app)
        }
        if app.flags.kill.load(std::sync::atomic::Ordering::SeqCst) {
            if fs::remove_file("/tmp/seppun-kb.pid").is_ok() {
                return println!("seppun-kb stopped");
            }
            ungrab_all_binds(app);
            eprintln!("Failed to delete seppun-kb pid file");
            break;
        }

        match app.connection.wait_for_event().unwrap() {
            Event::XkbStateNotify(event) => {
                if i32::from(event.device_id) == app.xkb.device_id {
                    app.xkb.state.update_mask(
                        event.base_mods.into(),
                        event.latched_mods.into(),
                        event.locked_mods.into(),
                        event.base_group.try_into().unwrap(),
                        event.latched_group.try_into().unwrap(),
                        event.locked_group.into(),
                    );
                }
            }
            Event::KeyPress(event) => {
                let sym = app.xkb.state.key_get_one_sym(event.detail.into());
                for bind in app.config.binds.iter() {
                    if event.state == bind.keybutmask && bind.key == sym {
                        if let Some(cmd) = &bind.cmd {
                            if cmd.is_empty() {
                                println!("No command given")
                            } else {
                                match Command::new(&cmd[0]).args(&cmd[1..]).spawn() {
                                    Ok(_) => {}
                                    Err(err) => eprintln!("Failed to run command: {err:?}"),
                                }
                            }
                        }
                    }
                }
            }
            event => println!("Ignoring event {event:?}"),
        }
        app.connection.flush().expect("Failed to flush x server");
    }
}
fn grab_all_binds(app: &App) {
    for bind in app.config.binds.iter() {
        for keycode in app.xkb.keymap.min_keycode().raw()..app.xkb.keymap.max_keycode().raw() {
            let keysyms = app.xkb.keymap.key_get_syms_by_level(keycode.into(), 0, 0);
            for key in keysyms {
                if (format!("{:?}", key)) == bind.key.name().unwrap() {
                    match xproto::grab_key(
                        &app.connection,
                        true,
                        app.root_window,
                        bind.mods,
                        Keycode::from(keycode),
                        GrabMode::ASYNC,
                        GrabMode::ASYNC,
                    ) {
                        Ok(cookie) => match cookie.check() {
                            Ok(_) => {}
                            Err(err) => {
                                eprintln!("Failed to grab {:?},  {:?}, {err:?}", keycode, bind.mods)
                            }
                        },
                        Err(err) => eprintln!("{err:?}"),
                    }
                }
            }
        }
    }
}

fn ungrab_all_binds(app: &App) {
    for bind in app.config.binds.iter() {
        for keycode in app.xkb.keymap.min_keycode().raw()..app.xkb.keymap.max_keycode().raw() {
            let keysyms = app.xkb.keymap.key_get_syms_by_level(keycode.into(), 0, 0);
            for key in keysyms {
                if (format!("{:?}", key)) == bind.key.name().unwrap() {
                    match xproto::ungrab_key(
                        &app.connection,
                        Keycode::from(keycode),
                        app.root_window,
                        bind.mods,
                    ) {
                        Ok(cookie) => match cookie.check() {
                            Ok(_) => {}
                            Err(err) => {
                                eprintln!(
                                    "Failed to ungrab {:?},  {:?}, {err:?}",
                                    keycode, bind.mods
                                )
                            }
                        },
                        Err(err) => eprintln!("{err:?}"),
                    }
                }
            }
        }
    }
}

fn start_daemon() {
    let stdout = File::create("/tmp/seppun-kb.out").unwrap();
    let stderr = File::create("/tmp/seppun-kb.err").unwrap();

    let daemon = Daemonize::new()
        .pid_file("/tmp/seppun-kb.pid")
        .stdout(stdout)
        .stderr(stderr);

    match daemon.start() {
        Ok(_) => {
            let mut app = match init_app() {
                Ok(app) => app,
                Err(err) => {
                    panic!("Failed to open the app: {err:?}")
                }
            };

            let app_flags_clone = app.flags.clone();
            thread::spawn(move || {
                let mut signals =
                    Signals::new([SIGHUP, SIGTERM]).expect("Failed to create signal handler");
                for signal in signals.forever() {
                    if signal == SIGHUP {
                        app_flags_clone
                            .reload
                            .store(true, std::sync::atomic::Ordering::SeqCst)
                    } else if signal == SIGTERM {
                        app_flags_clone
                            .kill
                            .store(true, std::sync::atomic::Ordering::SeqCst)
                    }
                }
            });

            grab_all_binds(&app);
            handle_app(&mut app);
        }
        Err(e) => {
            eprintln!("Error: {e:?}");
        }
    }
}

/// Stop the daemon
fn stop_signal_then_clean() {
    if let Ok(pid) = fs::read_to_string("/tmp/seppun-kb.pid") {
        if let Ok(pid) = pid.trim().parse::<i32>() {
            match kill(Pid::from_raw(pid), Signal::SIGTERM) {
                Err(err) => {
                    eprintln!("Failed to send SIGTERM to seppun-kb.pid \n {err:?}");
                }
                Ok(_) => {
                    println!("seppun-kb procces now should be killed")
                }
            }
            return;
        }
    }
    eprintln!("Failed to stop daemon: Daemon is not running or PID file is missing.");
    exit(0);
}
fn reload_signal() {
    if let Ok(pid) = fs::read_to_string("/tmp/seppun-kb.pid") {
        if let Ok(pid) = pid.trim().parse::<i32>() {
            match kill(Pid::from_raw(pid), Signal::SIGHUP) {
                Err(err) => {
                    eprintln!("Failed to send SIGHUP to PID {pid}: {err}");
                }
                Ok(_) => {
                    println!("Sent SIGHUP to PID {pid}. Stopping daemon...");
                }
            }
            return;
        }
    }
    eprintln!("Failed to stop daemon: Daemon is not running or PID file is missing.");
    exit(0);
}
pub fn add_bind_then_reload(data: &str) {
    println!("{}", data);
    //check if is good
    if let Some(_bind) = plain_into_bind(data) {
        let mut config_file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(
                home_dir()
                    .expect("Failed to get the $HOME")
                    .join(".config/seppun/kb"),
            )
            .expect("Failed to open the config file");
        config_file
            .write_all(data.as_bytes())
            .expect("Failed writing to file");
        println!("Bind added: {}", data);
        reload_signal();
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Start => start_daemon(),
        Commands::Stop => stop_signal_then_clean(),
        Commands::Reload => reload_signal(),
        Commands::Add(args) => add_bind_then_reload(&args.data),
    }
    Ok(())
}
