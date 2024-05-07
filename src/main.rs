use std::path::PathBuf;
use tokio::{
    io::AsyncReadExt,
    net::windows::named_pipe::{ClientOptions, ServerOptions},
    process::Command,
    signal,
};
use windows::Win32::System::{
    Console::{GenerateConsoleCtrlEvent, SetConsoleCtrlHandler, CTRL_BREAK_EVENT},
    Threading::CREATE_NEW_PROCESS_GROUP,
};

fn send_segint(process_id: Option<u32>) {
    unsafe {
        if let Err(err) = SetConsoleCtrlHandler(None, true) {
            log::error!("{:?}", err);
        }
        if let Err(err) = GenerateConsoleCtrlEvent(CTRL_BREAK_EVENT, process_id.unwrap_or(0)) {
            log::error!("Failed to send break event");
            log::error!("{:?}", err);
        }
    };
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env = env_logger::Env::default()
        .filter_or("ONE_INSTANCE_LOG_LEVEL", "info")
        .write_style_or("ONE_INSTANCE_LOG_STYLE", "always");

    env_logger::Builder::from_env(env)
        .format_module_path(false)
        .format_target(false)
        .format_indent(None)
        .init();

    let args: Vec<String> = std::env::args().collect();
    let program = &args[1];

    let program_file_path = PathBuf::from(program);
    let pipe_name: String = format!(
        r"\\.\pipe\one_instance_{}",
        program_file_path
            .file_name()
            .map_or(None, |f| f.to_str())
            .expect("This is not a file")
            .to_string()
            .replace(".", "_")
    );

    if let Ok(mut client) = ClientOptions::new().open(&pipe_name) {
        let _ = client.read_i32().await;
    }

    let server = ServerOptions::new()
        .first_pipe_instance(true)
        .create(&pipe_name)?;

    let mut command = Command::new(program_file_path);
    command.creation_flags(CREATE_NEW_PROCESS_GROUP.0);
    if args.len() > 2 {
        command.args(&args[2..]);
    }

    let mut child = command.spawn().expect("Could not spawn process");

    let child_id = child.id();

    let mut ctrl_c_signal = signal::windows::ctrl_c()?;
    let mut ctrl_close_signal = signal::windows::ctrl_close()?;
    let mut ctrl_break_signal = signal::windows::ctrl_break()?;
    let mut ctrl_logoff_signal = signal::windows::ctrl_logoff()?;
    let mut ctrl_shutdown_signal = signal::windows::ctrl_shutdown()?;

    let succeded;
    tokio::select! {
        _ = ctrl_c_signal.recv() => { succeded = false; },
        _ = ctrl_close_signal.recv() => { succeded = false; },
        _ = ctrl_break_signal.recv() => { succeded = false; },
        _ = ctrl_logoff_signal.recv() => { succeded = false; },
        _ = ctrl_shutdown_signal.recv() => { succeded = false; },
        _ = server.connect() => { succeded = false; },
        _ = child.wait() => { succeded = true; },
    };

    if !succeded {
        send_segint(child_id);
        let _ = child.wait().await;
    }

    return Ok(());
}
