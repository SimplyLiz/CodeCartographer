from datetime import datetime, timezone


def inject_state():
    with open("state_key.md", "r", encoding="utf-8") as f:
        state_content = f.read()
    
    timestamp = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    
    print('<cmp_protocol version="0.1">')
    print("<meta>")
    print("<session_id>a1b2-c3d4</session_id>")
    print(f"<timestamp>{timestamp}</timestamp>")
    print("<compression_ratio>4.5x</compression_ratio>")
    print("</meta>")
    print("<state_key>")
    print(state_content)
    print("</state_key>")
    print("</cmp_protocol>")


if __name__ == "__main__":
    inject_state()
