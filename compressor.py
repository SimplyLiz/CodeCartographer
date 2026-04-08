import json
import os
import shutil
import subprocess
import sys


def get_cartographer_analysis(target: str) -> dict | None:
    """
    Run `cartographer deps <target> --format json` and return parsed JSON output.
    Returns None if cartographer is not available or command fails.
    """
    # Check if cartographer is in PATH
    if not shutil.which("cartographer"):
        print("Warning: 'cartographer' CLI not found in PATH. Skipping dependency analysis.")
        return None

    try:
        result = subprocess.run(
            ["cartographer", "deps", target, "--format", "json"],
            capture_output=True,
            text=True,
            timeout=30
        )

        if result.returncode != 0:
            print(f"Warning: cartographer command failed: {result.stderr.strip()}")
            return None

        return json.loads(result.stdout)

    except subprocess.TimeoutExpired:
        print("Warning: cartographer command timed out.")
        return None
    except json.JSONDecodeError as e:
        print(f"Warning: Failed to parse cartographer output as JSON: {e}")
        return None
    except Exception as e:
        print(f"Warning: Unexpected error running cartographer: {e}")
        return None


def deps_to_xml(deps_output: dict) -> str:
    """
    Convert cartographer deps JSON output to token-efficient XML format.
    """
    node_id = deps_output.get("node_id", "")
    node_name = deps_output.get("node_name", "unknown")
    dependencies = deps_output.get("dependencies", [])

    # Extract node type from node_id (e.g., "cls:path:Name" -> "class")
    node_type = "unknown"
    if node_id.startswith("cls:"):
        node_type = "class"
    elif node_id.startswith("fn:"):
        node_type = "function"
    elif node_id.startswith("mod:"):
        node_type = "module"

    # Extract file path from node_id
    parts = node_id.split(":")
    file_path = parts[1] if len(parts) > 1 else ""

    lines = ["<CURRENT_FOCUS>"]
    lines.append(f'  <NODE name="{node_name}" type="{node_type}" path="{file_path}">')

    if dependencies:
        lines.append(f'    <DEPS count="{len(dependencies)}">')
        for dep in dependencies:
            dep_name = dep.get("name", "")
            dep_type = dep.get("node_type", "")
            dep_path = dep.get("file_path", "")
            lines.append(f'      <DEP name="{dep_name}" type="{dep_type}" path="{dep_path}" />')
        lines.append("    </DEPS>")

    lines.append("  </NODE>")
    lines.append("</CURRENT_FOCUS>")

    return "\n".join(lines)


def compress_chat_log(target: str | None = None):
    """
    Generate state snapshot. If target is provided, includes cartographer dependency analysis.
    """
    output_parts = []

    # Run cartographer analysis if target provided
    if target:
        deps_output = get_cartographer_analysis(target)
        if deps_output:
            xml_block = deps_to_xml(deps_output)
            output_parts.append(xml_block)
        else:
            output_parts.append("<!-- cartographer analysis unavailable -->")

    # Write state key
    with open("state_key.md", "w", encoding="utf-8") as f:
        f.write("\n\n".join(output_parts) if output_parts else "<!-- No state captured -->")

    print("State snapshot saved to state_key.md")


if __name__ == "__main__":
    target_file = sys.argv[1] if len(sys.argv) > 1 else None
    compress_chat_log(target_file)
