import argparse
import json
import re
import secrets
from pathlib import Path


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Generate or derive Caesar trusted keys")
    sub = parser.add_subparsers(dest="command", required=True)

    generate = sub.add_parser("generate", help="Generate a new edge seed and public key")
    generate.add_argument("--hub-config", help="Optional hub config to update with the generated public key")

    derive = sub.add_parser("derive", help="Derive public key from existing edge seed")
    derive.add_argument("--seed-hex", required=True)
    derive.add_argument("--hub-config", help="Optional hub config to update with the derived public key")

    return parser.parse_args()


def main() -> int:
    try:
        from nacl.signing import SigningKey
    except ImportError as exc:
        raise SystemExit("PyNaCl is required. Install it with: python -m pip install pynacl") from exc

    args = parse_args()

    if args.command == "generate":
        seed = secrets.token_bytes(32)
    else:
        seed = bytes.fromhex(args.seed_hex)
        if len(seed) != 32:
            raise SystemExit("seed hex must decode to exactly 32 bytes")

    signing_key = SigningKey(seed)
    verify_key = signing_key.verify_key

    payload = {
        "seed_hex": seed.hex(),
        "public_key": verify_key.encode().hex(),
    }

    if getattr(args, "hub_config", None):
        update_hub_config(Path(args.hub_config), payload["public_key"])

    print(json.dumps(payload, indent=2))
    return 0


def update_hub_config(path: Path, public_key: str) -> None:
    raw = path.read_text(encoding="utf-8")
    match = re.search(r"^trusted_public_keys\s*=\s*\[(.*)\]\s*$", raw, re.MULTILINE)
    if not match:
        raise SystemExit(f"Could not find trusted_public_keys in {path}")

    current = match.group(1).strip()
    entries = []
    if current:
        entries = [value.strip().strip('"') for value in current.split(",") if value.strip()]
    if public_key not in entries:
        entries.append(public_key)
    replacement = 'trusted_public_keys = [{}]'.format(", ".join(f'"{item}"' for item in entries))
    updated = raw[: match.start()] + replacement + raw[match.end() :]
    path.write_text(updated, encoding="utf-8")


if __name__ == "__main__":
    raise SystemExit(main())
