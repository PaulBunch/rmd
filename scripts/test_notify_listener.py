#!/usr/bin/env python3
# pyright: reportMissingTypeStubs=false, reportUnknownMemberType=false, reportUnknownVariableType=false, reportUnknownParameterType=false, reportUnknownArgumentType=false, reportAttributeAccessIssue=false

"""
Mock D-Bus notification server for testing `rmd` without a real notification daemon.

Usage:
    python3 scripts/test_notify_listener.py
"""

from dasbus.connection import SessionMessageBus
from dasbus.loop import EventLoop
from dasbus.server.interface import dbus_interface
from dasbus.typing import Int32, Str, UInt32, Variant


@dbus_interface("org.freedesktop.Notifications")
class Notifications:
    """Mock implementation of org.freedesktop.Notifications D-Bus interface."""

    def Notify(
        self,
        app_name: Str,
        replaces_id: UInt32,
        app_icon: Str,
        summary: Str,
        body: Str,
        actions: list[Str],
        hints: dict[Str, Variant],
        expire_timeout: Int32,
    ) -> UInt32:
        """Receive notification and print to console."""
        print("--- Notification received ---")
        print(f"App:         {app_name}")
        print(f"Replaces ID: {replaces_id}")
        print(f"Icon:        {app_icon}")
        print(f"Summary:     {summary}")
        print(f"Body:        {body}")
        print(f"Actions:     {actions}")
        print(f"Timeout:     {expire_timeout} ms")
        print(f"Hints:       {hints}")
        print("-----------------------------")
        return UInt32(42)  # Return mock notification ID

    def GetCapabilities(self) -> list[Str]:
        """Return server capabilities."""
        return ["body", "actions"]

    def GetServerInformation(self) -> tuple[str, str, str, str]:
        """Return server identification information."""
        return ("rmd-test-listener", "rmd", "1.0", "1.2")


def main() -> None:
    bus = SessionMessageBus()
    bus.publish_object("/org/freedesktop/Notifications", Notifications())
    bus.register_service("org.rmd.test.Notifications")

    print("Listening on org.rmd.test.Notifications ...")
    print("Press Ctrl+C to exit.")
    loop = EventLoop()
    try:
        loop.run()
    except KeyboardInterrupt:
        print("\nStopped.")


if __name__ == "__main__":
    main()
