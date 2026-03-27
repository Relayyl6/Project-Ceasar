import json
import os
import time
from pathlib import Path

import rclpy
from rclpy.node import Node
from std_msgs.msg import String


class CaesarBridgeNode(Node):
    def __init__(self) -> None:
        super().__init__("caesar_bridge")
        self.declare_parameter("journal_path", "output/caesar/high_interest.jsonl")
        self.declare_parameter("topic_name", "caesar_tactical_intel")
        self.declare_parameter("poll_interval_ms", 500)

        journal_path = self.get_parameter("journal_path").get_parameter_value().string_value
        topic_name = self.get_parameter("topic_name").get_parameter_value().string_value
        poll_interval_ms = self.get_parameter("poll_interval_ms").get_parameter_value().integer_value

        self.journal_path = Path(journal_path)
        self.publisher = self.create_publisher(String, topic_name, 10)
        self._offset = 0
        self._inode = None
        self._timer = self.create_timer(poll_interval_ms / 1000.0, self._poll_journal)

        self.get_logger().info(f"Watching {self.journal_path} and publishing to {topic_name}")

    def _poll_journal(self) -> None:
        if not self.journal_path.exists():
            return

        stat = self.journal_path.stat()
        inode = (stat.st_ino, stat.st_size)
        if self._inode is not None and stat.st_size < self._offset:
            self._offset = 0
        self._inode = inode

        with self.journal_path.open("r", encoding="utf-8") as handle:
            handle.seek(self._offset)
            for line in handle:
                line = line.strip()
                if not line:
                    continue
                try:
                    record = json.loads(line)
                except json.JSONDecodeError as error:
                    self.get_logger().warning(f"Skipping malformed journal line: {error}")
                    continue
                msg = String()
                msg.data = json.dumps(record)
                self.publisher.publish(msg)
            self._offset = handle.tell()


def main() -> None:
    rclpy.init()
    node = CaesarBridgeNode()
    try:
        rclpy.spin(node)
    except KeyboardInterrupt:
        pass
    finally:
        node.destroy_node()
        rclpy.shutdown()


if __name__ == "__main__":
    main()
