use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::error::Error;
use console::style;

pub fn get_passop_dir() -> PathBuf {
    let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push(".config/passop");
    path
}

pub fn get_age_key_dir() -> PathBuf {
    let mut path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push(".config/sops/age");
    path
}

pub fn is_setup_complete() -> bool {
    let passop_dir = get_passop_dir();
    let key_file = get_age_key_dir().join("keys.txt");
    let secrets_file = passop_dir.join("secrets.enc.yaml");
    let sops_config = passop_dir.join(".sops.yaml");

    key_file.exists() && secrets_file.exists() && sops_config.exists()
}

pub fn run_setup() -> Result<(), Box<dyn Error>> {
    println!("{}", style("⚡ Welcome to PASSop Setup! ⚡\n").bold().cyan());

    // 1. Check Dependencies
    check_dependency("sops", "Please install SOPS: https://github.com/getsops/sops")?;
    check_dependency("age-keygen", "Please install age: https://github.com/FiloSottile/age")?;

    let passop_dir = get_passop_dir();
    let age_dir = get_age_key_dir();
    fs::create_dir_all(&passop_dir)?;
    fs::create_dir_all(&age_dir)?;

    let key_path = age_dir.join("keys.txt");

    // 2. Generate age key if missing
    if !key_path.exists() {
        println!("{}", style("🔑 No age key found. Generating a new one...").yellow());
        let output = Command::new("age-keygen")
            .arg("-o")
            .arg(&key_path)
            .output()?;

        if !output.status.success() {
            return Err("Failed to execute age-keygen. Is it installed?".into());
        }
    } else {
        println!("{}", style("✔ Existing age key detected.").green());
    }

    // 3. Extract public key for .sops.yaml config
    let key_content = fs::read_to_string(&key_path)?;
    let public_key = key_content
        .lines()
        .find(|line| line.starts_with("# public key: "))
        .map(|line| line.replace("# public key: ", ""))
        .ok_or("Could not find public key in keys.txt")?
        .trim()
        .to_string();

    // Set env var dynamically for the current runtime
    std::env::set_var("SOPS_AGE_KEY_FILE", &key_path);

    // 4. Create .sops.yaml in passop directory
    let sops_config_path = passop_dir.join(".sops.yaml");
    if !sops_config_path.exists() {
        println!("{}", style("📝 Creating SOPS configuration file...").yellow());
        let sops_yaml = format!(
            "creation_rules:\n  - path_regex: .*\\.enc\\.yaml$\n    age: \"{}\"\n",
            public_key
        );
        fs::write(&sops_config_path, sops_yaml)?;
    }

    // 5. Initialize the encrypted database
    let secrets_path = passop_dir.join("secrets.enc.yaml");
    if !secrets_path.exists() {
        println!("{}", style("🔒 Initializing encrypted password vault...").yellow());
        
        let mut child = Command::new("sops")
            .args([
                "--encrypt",
                "--config", sops_config_path.to_str().unwrap(),
                "--input-type", "yaml",
                "--output-type", "yaml",
                "--filename-override", secrets_path.to_str().unwrap(), // ⚡ Fix: Use absolute path
                "/dev/stdin",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        {
            use std::io::Write;
            let stdin = child.stdin.as_mut().ok_or("Failed to open stdin")?;
            stdin.write_all(b"{}")?; // Empty YAML Map
        }

        let output = child.wait_with_output()?;
        if !output.status.success() {
            return Err(format!(
                "SOPS failed to encrypt the database: {}",
                String::from_utf8_lossy(&output.stderr)
            ).into());
        }

        fs::write(&secrets_path, output.stdout)?;
    }

    println!("\n{}", style("🎉 PASSop is successfully configured and ready to go!").bold().green());
    println!("Vault location: {}", style(secrets_path.display()).italic());
    println!("Private Key:    {}\n", style(key_path.display()).italic());

    Ok(())
}

fn check_dependency(cmd: &str, install_instructions: &str) -> Result<(), Box<dyn Error>> {
    let check = if cfg!(target_os = "windows") {
        Command::new("where").arg(cmd).output()
    } else {
        Command::new("which").arg(cmd).output()
    };

    match check {
        Ok(output) if output.status.success() => Ok(()),
        _ => Err(format!(
            "Dependency missing: '{}' was not found in your PATH.\n{}",
            cmd, install_instructions
        ).into()),
    }
}
