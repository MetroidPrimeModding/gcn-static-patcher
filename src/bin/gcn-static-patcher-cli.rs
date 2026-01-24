use anyhow::Result;
use clap::Parser;
use gcn_static_patcher::{find_app_dir, load_mod_data, run_cli_mode, Args};

fn main() -> Result<()> {
  // Initialize logging
  let log_file_path = find_app_dir().join("patcher.log");
  println!("Log file path: {:?}", log_file_path);
  fern::Dispatch::new()
    .format(|out, message, record| {
      out.finish(format_args!(
        "{}[{}][{}] {}",
        chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
        record.target(),
        record.level(),
        message
      ))
    })
    .level(log::LevelFilter::Info)
    .chain(std::io::stdout())
    .chain(fern::log_file(log_file_path)?)
    .apply()?;


  let args = Args::parse();

  let mod_path = std::env::current_dir()?
    .join(&args.mod_file);

  let mod_data = load_mod_data(mod_path);

  let mod_data = mod_data?;
  run_cli_mode(&args, mod_data)?;

  Ok(())
}
