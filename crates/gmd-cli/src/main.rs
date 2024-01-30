use clap::{Parser, Subcommand};
use eyre::{Result, WrapErr};
use gmd_parser::{
    calculator::{GMDDay, GMDSummary},
    models::{GMDLog, LogEntry, StartDay},
    parser::FromGMD,
};
use itertools::Itertools;
use std::{iter::once, path::PathBuf};
use tap::prelude::*;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    files: Vec<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// does testing things
    Test {
        /// lists test values
        #[arg(short, long)]
        list: bool,
    },
}

fn setup_logging() {
    use tracing_subscriber::{prelude::*, EnvFilter};
    let subscriber = tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or(EnvFilter::from("info")))
        .with(tracing_subscriber::fmt::Layer::new().with_writer(std::io::stderr));
    if let Err(message) = tracing::subscriber::set_global_default(subscriber) {
        eprintln!("logging setup failed: {message:?}");
    }
}

fn main() -> Result<()> {
    setup_logging();
    color_eyre::install().ok();
    Cli::parse().pipe(|Cli { files }| {
        files
            .into_iter()
            .map(|path| {
                std::fs::read_to_string(&path)
                    .wrap_err("reading file")
                    .and_then(|contents| {
                        GMDLog::from_gmd(&contents)
                            .wrap_err("parsing file")
                            .with_context(|| format!("reading '{}'", path.display()))
                    })
            })
            .collect::<Result<Vec<_>>>()
            .context("file(s) corrupted")
            .and_then(|input| {
                input
                    .into_iter()
                    .sorted_by_key(|log| {
                        log.0.iter().find_map(|entry| match entry {
                            LogEntry::StartDay(StartDay(day)) => Some(*day),
                            _ => None,
                        })
                    })
                    .flat_map(|GMDLog(entries)| entries.into_iter())
                    .collect::<Vec<_>>()
                    .pipe(GMDLog)
                    .pipe(|log| {
                        log.pipe_ref(GMDSummary::from_log)
                            .map(|summary| {
                                summary
                                    .0
                                    .values()
                                    .flat_map(|day| day.state.iter())
                                    .sorted_unstable_by_key(|(_, quantity)| quantity.amount)
                                    .rev()
                                    .map(|(name, _)| name)
                                    .unique_by(|name| *name)
                                    .collect_vec()
                                    .pipe(|tracked_products| {
                                        once(
                                            once("day".to_string())
                                                .chain(
                                                    tracked_products
                                                        .iter()
                                                        .map(|name| name.to_string()),
                                                )
                                                .collect_vec(),
                                        )
                                        .chain(
                                            summary
                                                .0
                                                .iter()
                                                .map(|(day, GMDDay { state, .. })| {
                                                    once(day.to_string())
                                                        .chain(tracked_products.iter().map(
                                                            |product| {
                                                                state
                                                                    .get(product)
                                                                    .copied()
                                                                    .map(|v| v.to_string())
                                                                    .unwrap_or_else(|| "~".into())
                                                            },
                                                        ))
                                                        .collect_vec()
                                                })
                                                .collect_vec(),
                                        )
                                    })
                                    .pipe(tabled::tables::IterTable::new)
                                    .to_string()
                            })
                            .map(|table| {
                                println!("{table}");
                            })
                    })
            })
    })
}
