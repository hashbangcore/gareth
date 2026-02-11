mod core;
use core::interface;

use clap::{CommandFactory, Parser};
use clap_complete::generate;

fn main() {
    let directives = interface::CommandLineDirectives::parse();

    match &directives.command {
        Some(interface::Commands::Completion { shell }) => {
            let mut completion = interface::CommandLineDirectives::command();
            generate(*shell, &mut completion, "gareth", &mut std::io::stdout());
        }
        Some(interface::Commands::Prompt { items }) => {
            println!("ECHO: {}", items.join(" "))
        }
        None => {
            if !directives.prompt.is_empty() {
                println!("ECHO: {}", directives.prompt.join(" "));
            } else {
                println!("START CHAT");
            }
        }
    }
}
