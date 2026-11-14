#!/usr/bin/env python3
"""Interactive keyboard fixture: requests a keyboard protocol and shows the bytes each key sends."""

import argparse
import os
import sys
import termios
import tty

PROTOCOLS = {
    # (enable, disable)
    "legacy": ("", ""),
    "kitty": ("\x1b[>1u", "\x1b[<u"),
    "kitty-all": ("\x1b[>31u", "\x1b[<u"),
    "modify-other-keys": ("\x1b[>4;2m", "\x1b[>4m"),
}


def output(text):
    os.write(sys.stdout.fileno(), text.encode("utf-8"))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("protocol", choices=PROTOCOLS, nargs="?", default="kitty")
    args = parser.parse_args()
    if not sys.stdin.isatty() or not sys.stdout.isatty():
        parser.error("run this fixture inside an interactive terminal")
    enable, disable = PROTOCOLS[args.protocol]
    original = termios.tcgetattr(sys.stdin.fileno())
    try:
        tty.setraw(sys.stdin.fileno())
        output(enable)
        output(f"Keyboard fixture: {args.protocol}; q quits.\r\n")
        output("Try Ctrl-I vs Tab, Ctrl-M vs Enter, Shift-Enter, Ctrl-Enter, Escape, Alt-a, arrows.\r\n")
        while True:
            data = os.read(sys.stdin.fileno(), 4096)
            if not data or data == b"q" or data.startswith(b"\x1b[113"):
                return
            output(f"{data!r}  {data.hex(' ')}\r\n")
    finally:
        output(disable)
        termios.tcsetattr(sys.stdin.fileno(), termios.TCSANOW, original)


if __name__ == "__main__":
    main()
