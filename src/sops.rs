use std::collections::HashMap;
use std::error::Error;
use std::io::Write;
use std::process::{Command, Stdio};

pub struct SopsBackend {
    db_path: std::path::PathBuf,
    config_path: std::path::PathBuf,
}

impl SopsBackend {
    pub fn new() -> Self {
        let home = dirs::home_dir().expect("Could not find home directory");
        let app_dir = home.join(".config/passop");
        
        Self {
            db_path: app_dir.join("secrets.enc.yaml"),
            config_path: app_dir.join(".sops.yaml"),
        }
    }

    /// Read and decrypt the secrets from the database
    pub fn read_secrets(&self) -> Result<HashMap<String, String>, Box<dyn Error>> {
        if !self.db_path.exists() {
            return Ok(HashMap::new());
        }

        let output = Command::new("sops")
            .args([
                "--decrypt",
                "--config", self.config_path.to_str().unwrap(),
                self.db_path.to_str().unwrap(),
            ])
            .output()?;

        if !output.status.success() {
            let err_msg = String::from_utf8_lossy(&output.stderr);
            return Err(format!("SOPS decryption failed: {}", err_msg).into());
        }

        let secrets: HashMap<String, String> = serde_yaml::from_slice(&output.stdout)?;
        Ok(secrets)
    }

    /// Encrypt and write the secrets back to disk
    pub fn write_secrets(&self, secrets: &HashMap<String, String>) -> Result<(), Box<dyn Error>> {
        let yaml_data = serde_yaml::to_string(secrets)?;

        // Spawn SOPS as a child process, writing to stdin and capturing its stdout
        let mut child = Command::new("sops")
            .args([
                "--encrypt",
                "--config", self.config_path.to_str().unwrap(),
                "--input-type", "yaml",
                "--output-type", "yaml",
                "--filename-override", self.db_path.to_str().unwrap(), // ⚡ Absolute path fix
                "/dev/stdin",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        // Feed our YAML string into SOPS's stdin
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(yaml_data.as_bytes())?;
        }

        // Wait for SOPS to finish processing
        let output = child.wait_with_output()?;

        if !output.status.success() {
            let err_msg = String::from_utf8_lossy(&output.stderr);
            return Err(format!("SOPS encryption failed: {}", err_msg).into());
        }

        // Write the encrypted output we received from SOPS to our database file
        std::fs::create_dir_all(self.db_path.parent().unwrap())?;
        std::fs::write(&self.db_path, output.stdout)?;

        Ok(())
    }
}
