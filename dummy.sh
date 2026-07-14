#!/usr/bin/env bash

set -euo pipefail

# Define paths matching your PASSop installation
VAULT_DIR="$HOME/.config/passop"
VAULT_FILE="$VAULT_DIR/secrets.enc.yaml"
SOPS_CONFIG="$VAULT_DIR/.sops.yaml"

# Ensure directories and the SOPS config actually exist
if [ ! -f "$SOPS_CONFIG" ]; then
  echo "❌ Error: PASSop config not found at $SOPS_CONFIG."
  echo "Please run your Rust setup first so your age key is generated."
  exit 1
fi

# List of dummy services to generate passwords for
services=(
  "github.com"
  "gitlab.com"
  "google.com"
  "netflix.com"
  "spotify.com"
  "amazon.com"
  "reddit.com"
  "server.local.ssh"
  "router.admin"
  "database.prod.master"
)

# Helper function to generate a secure random alphanumeric password
generate_password() {
  # Generates a 20-character random alphanumeric string
  LC_ALL=C tr -dc 'A-Za-z0-9' </dev/urandom | head -c 20
}

echo "🔌 Reading current secrets (if any)..."
# Start with an empty YAML structure or decrypt existing file
if [ -f "$VAULT_FILE" ]; then
  # Decrypt existing file to a temp file safely
  temp_yaml=$(mktemp)
  trap 'rm -f "$temp_yaml"' EXIT
  sops --decrypt --config "$SOPS_CONFIG" "$VAULT_FILE" >"$temp_yaml"
else
  # Create empty YAML map if file doesn't exist yet
  temp_yaml=$(mktemp)
  trap 'rm -f "$temp_yaml"' EXIT
  echo "{}" >"$temp_yaml"
fi

echo "🔑 Generating dummy entries..."
for service in "${services[@]}"; do
  # Only add if it doesn't already exist in the YAML to prevent overwriting
  if grep -q "^$service:" "$temp_yaml" 2>/dev/null; then
    echo "   ℹ️  '$service' already exists. Skipping..."
  else
    password=$(generate_password)
    # Format as standard YAML key-value
    echo "$service: \"$password\"" >>"$temp_yaml"
    echo "   ✨ Generated dummy entry for: $service"
  fi
done

echo "🔒 Encrypting database using SOPS..."
# Perform the encryption using the absolute path filename-override fix!
mkdir -p "$VAULT_DIR"
sops --encrypt \
  --config "$SOPS_CONFIG" \
  --input-type yaml \
  --output-type yaml \
  --filename-override "$VAULT_FILE" \
  "$temp_yaml" >"$VAULT_FILE"

echo -e "\n\033[32m✔ Done! Dummy entries successfully written to $VAULT_FILE\033[0m"
