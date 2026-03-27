# ROS 2 Caesar Bridge

This folder is a colcon-style ROS 2 workspace scaffold for the Caesar bridge package.

## What it does

The bridge tails the Caesar hub high-interest journal and republishes each JSON record to a ROS 2 topic using `std_msgs/String`.

## Package path

- [ros2_ws/src/caesar_bridge/package.xml](/C:/Users/Leonard/Documents/New project/ros2_ws/src/caesar_bridge/package.xml)
- [ros2_ws/src/caesar_bridge/caesar_bridge/bridge_node.py](/C:/Users/Leonard/Documents/New project/ros2_ws/src/caesar_bridge/caesar_bridge/bridge_node.py)

## Run inside ROS 2 Humble

```bash
cd ros2_ws
source /opt/ros/humble/setup.bash
colcon build
source install/setup.bash
ros2 run caesar_bridge bridge_node
```
