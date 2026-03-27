#!/usr/bin/env bash
set -euo pipefail

echo "[bootstrap-ros2] This script targets Ubuntu 22.04 with ROS 2 Humble"

sudo apt-get update
sudo apt-get install -y curl gnupg lsb-release software-properties-common

sudo curl -sSL https://raw.githubusercontent.com/ros/rosdistro/master/ros.key \
  -o /usr/share/keyrings/ros-archive-keyring.gpg

echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/ros-archive-keyring.gpg] http://packages.ros.org/ros2/ubuntu $(. /etc/os-release && echo "$UBUNTU_CODENAME") main" \
  | sudo tee /etc/apt/sources.list.d/ros2.list >/dev/null

sudo apt-get update
sudo apt-get install -y ros-humble-ros-base python3-colcon-common-extensions python3-rosdep

if command -v rosdep >/dev/null 2>&1; then
  sudo rosdep init || true
  rosdep update || true
fi

echo "[bootstrap-ros2] Next actions:"
echo "  1. source /opt/ros/humble/setup.bash"
echo "  2. cd ros2_ws"
echo "  3. colcon build"
echo "  4. source install/setup.bash"
echo "  5. ros2 run caesar_bridge bridge_node"
