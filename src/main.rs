use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use serde_yaml::Value;

use yamlq::{ops, path};

/// A yq-style YAML query/transform tool: get, set, and delete values in a
/// YAML file with a dot/bracket path (`.a.b[0].c`).
#[derive(Parser)]
#[command(name = "yamlq", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Print the value at PATH.
    Get { path: String, file: PathBuf },

    /// Set the value at PATH. Prints the updated document to stdout unless
    /// --in-place is given.
    Set {
        path: String,
        value: String,
        file: PathBuf,

        /// Write the result back to FILE instead of printing to stdout.
        #[arg(short = 'i', long = "in-place")]
        in_place: bool,

        /// Treat VALUE as a literal string instead of YAML-type-inferring
        /// it (so `set .port "8080" -r` stays the string "8080", not the
        /// number 8080).
        #[arg(short = 'r', long)]
        raw: bool,
    },

    /// Delete the value at PATH. Prints the updated document to stdout
    /// unless --in-place is given.
    Delete {
        path: String,
        file: PathBuf,

        /// Write the result back to FILE instead of printing to stdout.
        #[arg(short = 'i', long = "in-place")]
        in_place: bool,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Get { path, file } => cmd_get(&path, &file),
        Command::Set {
            path,
            value,
            file,
            in_place,
            raw,
        } => cmd_set(&path, &value, &file, in_place, raw),
        Command::Delete {
            path,
            file,
            in_place,
        } => cmd_delete(&path, &file, in_place),
    }
}

fn load(file: &Path) -> anyhow::Result<Value> {
    let text = std::fs::read_to_string(file)
        .map_err(|e| anyhow::anyhow!("reading {}: {e}", file.display()))?;
    serde_yaml::from_str_value_preserve(&text)
        .map_err(|e| anyhow::anyhow!("parsing {} as YAML: {e}", file.display()))
}

fn print_value(v: &Value) {
    match v {
        Value::String(s, _) => println!("{s}"),
        Value::Bool(b, _) => println!("{b}"),
        Value::Null(_) => println!("null"),
        Value::Number(n, _) => println!("{n}"),
        Value::Alias(a) => println!("*{a}"),
        other => {
            let rendered =
                serde_yaml::to_string(other).expect("re-serializing an already-parsed value");
            print!("{rendered}");
        }
    }
}

fn cmd_get(raw_path: &str, file: &Path) -> anyhow::Result<()> {
    let doc = load(file)?;
    let segments = path::parse(raw_path).map_err(|e| anyhow::anyhow!("{raw_path}: {e}"))?;
    let value = ops::get(&doc, &segments).map_err(|e| anyhow::anyhow!("{raw_path}: {e}"))?;
    print_value(value);
    Ok(())
}

fn cmd_set(
    raw_path: &str,
    raw_value: &str,
    file: &Path,
    in_place: bool,
    raw: bool,
) -> anyhow::Result<()> {
    let mut doc = load(file)?;
    let segments = path::parse(raw_path).map_err(|e| anyhow::anyhow!("{raw_path}: {e}"))?;

    let new_value = if raw {
        Value::String(raw_value.to_string(), None)
    } else {
        serde_yaml::from_str_value_preserve(raw_value).unwrap_or_else(|_| Value::String(raw_value.to_string(), None))
    };

    ops::set(&mut doc, &segments, new_value).map_err(|e| anyhow::anyhow!("{raw_path}: {e}"))?;
    write_result(&doc, file, in_place)
}

fn cmd_delete(raw_path: &str, file: &Path, in_place: bool) -> anyhow::Result<()> {
    let mut doc = load(file)?;
    let segments = path::parse(raw_path).map_err(|e| anyhow::anyhow!("{raw_path}: {e}"))?;
    ops::delete(&mut doc, &segments).map_err(|e| anyhow::anyhow!("{raw_path}: {e}"))?;
    write_result(&doc, file, in_place)
}

fn write_result(doc: &Value, file: &Path, in_place: bool) -> anyhow::Result<()> {
    let rendered =
        serde_yaml::to_string(doc).map_err(|e| anyhow::anyhow!("serializing result: {e}"))?;
    if in_place {
        std::fs::write(file, rendered)
            .map_err(|e| anyhow::anyhow!("writing {}: {e}", file.display()))?;
    } else {
        print!("{rendered}");
    }
    Ok(())
}
