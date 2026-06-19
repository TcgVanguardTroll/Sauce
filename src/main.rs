// CLI handlers naturally take several arguments (one per flag); the `Find`
// subcommand in particular carries many optional filters.
#![allow(clippy::too_many_arguments)]

mod cli;

fn main() -> anyhow::Result<()> {
    cli::run()
}
