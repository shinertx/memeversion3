#!/bin/bash
set -e

# MemeSnipe v24 - GCP VM Deployment Script
# This script creates a GCP VM, installs Docker, and deploys the system.

# --- Configuration ---
PROJECT_ID=$(gcloud config get-value project)
VM_NAME="meme-snipe-v24-vm"
ZONE="us-central1-a"
MACHINE_TYPE="e2-standard-4"
DISK_SIZE="30GB"
IMAGE_FAMILY="debian-11"
IMAGE_PROJECT="debian-cloud"
REPO_DIR="/opt/meme-snipe-v24"

source .env

echo "🚀 Deploying MemeSnipe v24 - The Live Simulation Engine to GCP..."
echo "Project: $PROJECT_ID | VM: $VM_NAME | Zone: $ZONE"

# --- Check for required files ---
if [ ! -f ".env" ]; then
    echo "❌ '.env' file not found! Please copy .env.example to .env and fill in your API keys."
    exit 1
fi
if [ ! -f "$WALLET_KEYPAIR_FILENAME" ] || [ ! -f "$JITO_AUTH_KEYPAIR_FILENAME" ]; then
    echo "❌ Wallet files missing! Ensure '$WALLET_KEYPAIR_FILENAME' and '$JITO_AUTH_KEYPAIR_FILENAME' exist in the project root."
    exit 1
fi

# --- Create or Update VM ---
if gcloud compute instances describe "$VM_NAME" --zone="$ZONE" --quiet &>/dev/null; then
    echo "⚠️ VM '$VM_NAME' already exists. Updating code and restarting services..."
    gcloud compute ssh "$VM_NAME" --zone="$ZONE" --command="sudo usermod -aG docker \$(whoami) || true"
    gcloud compute ssh "$VM_NAME" --zone="$ZONE" --command="sudo rm -rf $REPO_DIR"
    gcloud compute ssh "$VM_NAME" --zone="$ZONE" --command="sudo mkdir -p $REPO_DIR && sudo chown -R \$(whoami):\$(whoami) $REPO_DIR"
    
    echo "📦 Creating tarball of the project..."
    tar -czf memev25.tar.gz . --exclude='.git' --exclude='target' --exclude='__pycache__'
    gcloud compute scp memev25.tar.gz "$VM_NAME":~ --zone="$ZONE"
    gcloud compute ssh "$VM_NAME" --zone="$ZONE" --command="tar -xzf memev25.tar.gz -C $REPO_DIR && rm memev25.tar.gz"
    rm memev25.tar.gz
    
    echo "🐳 Building and deploying Docker services..."
    gcloud compute ssh "$VM_NAME" --zone="$ZONE" --command="cd $REPO_DIR && export DOCKER_BUILDKIT=1 && sudo -E docker compose up -d --build"
else
    echo "🔨 Creating new VM '$VM_NAME'..."
    gcloud compute instances create "$VM_NAME" \
        --project="$PROJECT_ID" \
        --zone="$ZONE" \
        --machine-type="$MACHINE_TYPE" \
        --boot-disk-size="$DISK_SIZE" \
        --image-family="$IMAGE_FAMILY" \
        --image-project="$IMAGE_PROJECT" \
        --tags=http-server,https-server \
        --metadata=startup-script='#! /bin/bash
            sudo apt-get update
            sudo apt-get install -y apt-transport-https ca-certificates curl gnupg lsb-release git
            curl -fsSL https://download.docker.com/linux/debian/gpg | sudo gpg --dearmor -o /usr/share/keyrings/docker-archive-keyring.gpg
            echo "deb [arch=amd64 signed-by=/usr/share/keyrings/docker-archive-keyring.gpg] https://download.docker.com/linux/debian $(lsb_release -cs) stable" | sudo tee /etc/apt/sources.list.d/docker.list > /dev/null
            sudo apt-get update
            sudo apt-get install -y docker-ce docker-ce-cli containerd.io docker-compose-plugin
            sudo usermod -aG docker $USER
            echo "✅ Docker installed."
            # Enable BuildKit
            sudo mkdir -p /etc/docker
            echo "{\"features\": {\"buildkit\": true}}" | sudo tee /etc/docker/daemon.json
            sudo systemctl restart docker
        '
    
    echo "⏳ Waiting for VM to be ready and Docker to install (approx. 90 seconds)..."
    sleep 90
    
    echo "📁 Creating repo directory and copying files..."
    gcloud compute ssh "$VM_NAME" --zone="$ZONE" --command="sudo mkdir -p $REPO_DIR && sudo chown \$(whoami):\$(whoami) $REPO_DIR"
    
    echo "📦 Creating tarball of the project..."
    tar -czf memev25.tar.gz . --exclude='.git' --exclude='target' --exclude='__pycache__'
    gcloud compute scp memev25.tar.gz "$VM_NAME":~ --zone="$ZONE"
    gcloud compute ssh "$VM_NAME" --zone="$ZONE" --command="tar -xzf memev25.tar.gz -C $REPO_DIR && rm memev25.tar.gz"
    rm memev25.tar.gz
    
    echo "🐳 Building and deploying Docker services..."
    gcloud compute ssh "$VM_NAME" --zone="$ZONE" --command="cd $REPO_DIR && export DOCKER_BUILDKIT=1 && sudo -E docker compose up -d --build"
fi

# --- Firewall Rules ---
FIREWALL_RULE_NAME="meme-snipe-v24-access"
if ! gcloud compute firewall-rules describe "$FIREWALL_RULE_NAME" --quiet &>/dev/null; then
    echo "🔥 Creating firewall rule '$FIREWALL_RULE_NAME'..."
    gcloud compute firewall-rules create "$FIREWALL_RULE_NAME" \
        --allow=tcp:8080,tcp:9090,tcp:3000,tcp:9184 \
        --description="Allow access to MemeSnipe v24 services" \
        --target-tags=http-server
fi

# --- Final Output ---
EXTERNAL_IP=$(gcloud compute instances describe "$VM_NAME" --zone="$ZONE" --format="get(networkInterfaces[0].accessConfigs[0].natIP)")

echo ""
echo "🎉 DEPLOYMENT COMPLETE!"
echo "----------------------------------------"
echo "📊 Dashboard: http://$EXTERNAL_IP:8080"
echo "📈 Prometheus: http://$EXTERNAL_IP:9090"
echo "📉 Grafana: http://$EXTERNAL_IP:3000"
echo "🔍 Executor Metrics: http://$EXTERNAL_IP:9184"
echo "----------------------------------------"
echo "SSH Access: gcloud compute ssh $VM_NAME --zone=$ZONE"
echo "View Logs: gcloud compute ssh $VM_NAME --zone=$ZONE --command='cd $REPO_DIR && sudo docker compose logs -f'"
echo ""
echo "⚠️  REMEMBER: System starts in PAPER_TRADING_MODE=true"
echo "To go live, update .env and restart services"
