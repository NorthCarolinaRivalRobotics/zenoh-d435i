#!/bin/bash
set -e # Exit immediately if a command exits with a non-zero status.
sudo apt-get update && sudo apt-get upgrade && sudo apt-get dist-upgrade
sudo apt update
sudo apt install v4l-utils
# Ensure udev is present in the container so that /etc/udev/rules.d exists
sudo apt-get install --yes --no-install-recommends udev
# Create the rules directory in case the image still does not provide it
sudo mkdir -p /etc/udev/rules.d
echo "Starting librealsense2 SDK installation (kernel patches will be skipped)..."

# 1. Update and upgrade system packages
echo "Updating and upgrading system packages..."
sudo apt-get update --fix-missing && sudo apt-get upgrade -y --fix-missing && sudo apt-get dist-upgrade -y --fix-missing
# 0.  Always fail fast
set -euo pipefail

sudo rm -rf /var/lib/apt/lists/*
sudo apt-get clean
sudo apt-get update -o Acquire::CompressionTypes::Order::=gz



# 2.  Refresh the index with a robust set of options
sudo apt-get update \
  -o Acquire::Retries=3 \
  -o Acquire::http::Pipeline-Depth=0 \
  -o Acquire::CompressionTypes::Order::=gz
  

# 3.  Install dependencies *in the same layer*
sudo DEBIAN_FRONTEND=noninteractive \
     apt-get install --yes --no-install-recommends \
     libssl-dev libusb-1.0-0-dev libudev-dev pkg-config \
     libgtk-3-dev git wget cmake build-essential \
     libglfw3-dev libgl1-mesa-dev libglu1-mesa-dev
# 3. Clone librealsense repository
echo "Cloning librealsense repository to /tmp/librealsense..."
git clone https://github.com/IntelRealSense/librealsense.git /tmp/librealsense

# 4. Setup udev rules (Note: Kernel patching is skipped as it's not feasible in a standard dev container)
echo "Setting up udev rules..."
cd /tmp/librealsense
./scripts/setup_udev_rules.sh

# 5. Build and install the librealsense2 SDK
echo "Building librealsense SDK..."
cd /tmp/librealsense
mkdir -p build && cd build
echo "Configuring build (Release mode, with non-graphical examples)..."
cmake .. -DCMAKE_BUILD_TYPE=Release
echo "Uninstalling any previous version, cleaning, compiling, and installing SDK..."
sudo make uninstall
make clean
make -j$(($(nproc)-1)) # Compile using nproc-1 cores
sudo make install      # Install the SDK

# 6. Cleanup
echo "Cleaning up temporary installation files..."
rm -rf /tmp/librealsense

echo "librealsense2 SDK installation complete."
echo "Note: Kernel patches were skipped. Direct camera operation from within the container might be limited." 