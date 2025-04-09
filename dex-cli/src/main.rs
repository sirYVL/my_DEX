// dex-cli/src/main.rs
use clap::{Parser, Subcommand};
use anyhow::Result;
use dex_core::{Order, Asset};

#[derive(Parser)]
#[command(name="dex-cli",version="0.1")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Add {
        order_id: String,
        user_id: String,
        amount: f64,
        price: f64,
    },
    Remove {
        order_id: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Add { order_id, user_id, amount, price } => {
            // Sende an Node => z.B. RPC / REST / Socket
            println!("Would add order {} user={} amt={} price={}", order_id, user_id, amount, price);
        },
        Commands::Remove { order_id } => {
            // Sende Remove an Node
            println!("Would remove order {}", order_id);
        },
    }
    Ok(())
}
