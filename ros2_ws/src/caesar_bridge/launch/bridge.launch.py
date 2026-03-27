from launch import LaunchDescription
from launch_ros.actions import Node


def generate_launch_description():
    return LaunchDescription(
        [
            Node(
                package="caesar_bridge",
                executable="bridge_node",
                name="caesar_bridge",
                parameters=[
                    {
                        "journal_path": "output/caesar/high_interest.jsonl",
                        "topic_name": "caesar_tactical_intel",
                        "poll_interval_ms": 500,
                    }
                ],
            )
        ]
    )
