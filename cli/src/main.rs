use clap::{Parser, Subcommand};

pub mod migrate;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
	#[command(subcommand)]
	command: Commands,
}

#[derive(Subcommand)]
enum Commands {
	#[clap(about = "Run database migrations")]
	Migrate,
}

fn main() -> Result<(), anyhow::Error> {
	let args = Args::parse();

	match args.command {
		Commands::Migrate => migrate::migrate()?,
	}

	Ok(())
}
