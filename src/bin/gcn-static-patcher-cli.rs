use anyhow::Result;
use clap::Parser;
use gcn_static_patcher::{Args, find_app_dir, load_mod_data, run_cli_mode};

fn main() -> Result<()> {
  // Initialize logging
  env_logger::Builder::from_default_env()
    .target(env_logger::Target::Stdout)
    .filter_level(log::LevelFilter::Info)
    .init();

  let args = Args::parse();

  let  mod_path = std::env::current_dir()?
    .join(&args.mod_file);

  let mod_data = load_mod_data(mod_path);

  let mod_data = mod_data?;
  run_cli_mode(&args, mod_data)?;

  Ok(())
}
