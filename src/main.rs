mod sops;
mod setup;

use clap::{Parser, Subcommand};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Password, FuzzySelect};
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
use sops::SopsBackend;
use qrcode::QrCode;
use qrcode::render::unicode;
use std::error::Error;

#[derive(Parser)]
#[command(name = "po")]
#[command(about = "PASSop: A SOPS-backed password manager", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Direct shortcut to fetch a password (acts like "call")
    name: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize the key files, directories, and database
    Setup,
    /// Add a new password entry
    Add { name: String },
    /// Retrieve a password (uses interactive search if name is omitted)
    Call { name: Option<String> },
    /// Remove a password entry (uses interactive search if name is omitted)
    Remove { name: Option<String> }, // ⚡ Changed to Option
    /// Edit an existing password (uses interactive search if name is omitted)
    Edit { name: Option<String> },   // ⚡ Changed to Option
    /// Generate a 2FA QR code and ID
    TwoFactor,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    // 1. Auto-trigger setup if it hasn't been completed yet
    if !setup::is_setup_complete() {
        match cli.command {
            Some(Commands::Setup) => {} // Let it fall through to manual run below
            _ => {
                println!("PASSop configuration not found. Initiating auto-setup...\n");
                setup::run_setup()?;
                println!("Auto-setup finished! Re-run your command to start using PASSop.");
                return Ok(());
            }
        }
    }

    let backend = SopsBackend::new();

    let command = cli.command.unwrap_or_else(|| {
        Commands::Call { name: cli.name }
    });

    match command {
        Commands::Setup => setup::run_setup()?,
        Commands::Add { name } => handle_add(&backend, name)?,
        Commands::Call { name } => handle_call(&backend, name)?,
        Commands::Remove { name } => handle_remove(&backend, name)?,
        Commands::Edit { name } => handle_edit(&backend, name)?,
        Commands::TwoFactor => handle_2fa()?,
    }

    Ok(())
}

fn generate_password() -> String {
    thread_rng()
        .sample_iter(&Alphanumeric)
        .take(20)
        .map(char::from)
        .collect()
}

fn handle_add(backend: &SopsBackend, name: String) -> Result<(), Box<dyn Error>> {
    let mut secrets = backend.read_secrets().unwrap_or_default();

    let generate = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt("Generate password?")
        .default(true)
        .interact()?;

    let final_password = if generate {
        let generated = generate_password();
        println!("\nGenerated password: \x1b[33m{}\x1b[0m", generated);
        generated
    } else {
        Password::with_theme(&ColorfulTheme::default())
            .with_prompt("Enter password")
            .interact()?
    };

    secrets.insert(name.clone(), final_password);
    if let Err(e) = backend.write_secrets(&secrets) {
        eprintln!("Error saving secrets to SOPS: {}", e);
    } else {
        println!("\x1b[32m✔ Password for '{}' was saved successfully!\x1b[0m", name);
    }

    Ok(())
}

fn handle_call(backend: &SopsBackend, name: Option<String>) -> Result<(), Box<dyn Error>> {
    let secrets = backend.read_secrets().unwrap_or_default();

    if secrets.is_empty() {
        println!("Your password vault is currently empty.");
        return Ok(());
    }

    let target_name = match name {
        Some(n) => n,
        None => {
            let keys: Vec<&String> = secrets.keys().collect();
            let selection = FuzzySelect::with_theme(&ColorfulTheme::default())
                .with_prompt("Search for a password")
                .items(&keys)
                .default(0)
                .interact_opt()?;

            match selection {
                Some(index) => keys[index].clone(),
                None => return Ok(()), // User canceled
            }
        }
    };

    match secrets.get(&target_name) {
        Some(password) => {
            println!("\n🔑 \x1b[1m{}\x1b[0m: \x1b[36m{}\x1b[0m", target_name, password);
        }
        None => {
            println!("\x1b[31mNo entry found for '{}'.\x1b[0m", target_name);
        }
    }

    Ok(())
}

fn handle_remove(backend: &SopsBackend, name: Option<String>) -> Result<(), Box<dyn Error>> {
    let mut secrets = backend.read_secrets().unwrap_or_default();

    if secrets.is_empty() {
        println!("Your password vault is currently empty.");
        return Ok(());
    }

    // Resolve target using CLI argument or FuzzySelect fallback
    let target_name = match name {
        Some(n) => n,
        None => {
            let keys: Vec<&String> = secrets.keys().collect();
            let selection = FuzzySelect::with_theme(&ColorfulTheme::default())
                .with_prompt("Select password to remove")
                .items(&keys)
                .default(0)
                .interact_opt()?;

            match selection {
                Some(index) => keys[index].clone(),
                None => return Ok(()), // User canceled
            }
        }
    };

    if !secrets.contains_key(&target_name) {
        println!("\x1b[31mNo entry found for '{}'.\x1b[0m", target_name);
        return Ok(());
    }

    let confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("Are you sure you want to remove your password for {}?", target_name))
        .default(false)
        .interact()?;

    if confirm {
        secrets.remove(&target_name);
        if let Err(e) = backend.write_secrets(&secrets) {
            eprintln!("Error saving changes: {}", e);
        } else {
            println!("\x1b[32mPassword for '{}' has been removed.\x1b[0m", target_name);
        }
    } else {
        println!("Canceled.");
    }

    Ok(())
}

fn handle_edit(backend: &SopsBackend, name: Option<String>) -> Result<(), Box<dyn Error>> {
    let mut secrets = backend.read_secrets().unwrap_or_default();

    if secrets.is_empty() {
        println!("Your password vault is currently empty.");
        return Ok(());
    }

    // Resolve target using CLI argument or FuzzySelect fallback
    let target_name = match name {
        Some(n) => n,
        None => {
            let keys: Vec<&String> = secrets.keys().collect();
            let selection = FuzzySelect::with_theme(&ColorfulTheme::default())
                .with_prompt("Select password to edit")
                .items(&keys)
                .default(0)
                .interact_opt()?;

            match selection {
                Some(index) => keys[index].clone(),
                None => return Ok(()), // User canceled
            }
        }
    };

    if !secrets.contains_key(&target_name) {
        println!("\x1b[31mNo entry found for '{}'. Use 'add' instead.\x1b[0m", target_name);
        return Ok(());
    }

    let new_password: String = Input::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("What do you want to change the password for '{}' to?", target_name))
        .interact_text()?;

    let confirm = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("Do you want to change the password to '{}'?", new_password))
        .default(true)
        .interact()?;

    if confirm {
        secrets.insert(target_name, new_password);
        if let Err(e) = backend.write_secrets(&secrets) {
            eprintln!("Error: {}", e);
        } else {
            println!("\x1b[32mPassword changed successfully!\x1b[0m");
        }
    } else {
        println!("\x1b[33mChange canceled!\x1b[0m");
    }

    Ok(())
}

fn handle_2fa() -> Result<(), Box<dyn Error>> {
    let raw_secret = "JBSWY3DPEHPK3PXP"; 
    let issuer = "PASSop";
    let account = "user@domain.com";

    let auth_url = format!(
        "otpauth://totp/{}:{}?secret={}&issuer={}",
        issuer, account, raw_secret, issuer
    );

    let code = QrCode::new(&auth_url).unwrap();
    let image = code.render::<unicode::Dense1x2>()
        .dark_color(unicode::Dense1x2::Dark)
        .light_color(unicode::Dense1x2::Light)
        .build();

    println!("\nScan this QR code with your authenticator app:");
    println!("{}", image);
    println!("Secret Key (Number ID): \x1b[33m{}\x1b[0m\n", raw_secret);

    println!("\x1b[31mPress ESC to exit!\x1b[0m");

    let term = console::Term::stdout();
    loop {
        if let Ok(console::Key::Escape) = term.read_key() {
            break;
        }
    }

    Ok(())
}
