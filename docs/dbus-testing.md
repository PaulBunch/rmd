# Manual Verification of D-Bus Notifications

To test how `rmd` works with D-Bus notifications (without installing a real notification server like Dunst, Mako, or fnott), you can run a mock server in Python using the `dasbus` library.

---

## 1. Environment Setup

Install the `dasbus` dependency:

```bash
pip install dasbus
# Or via your system package manager (e.g., Arch Linux):
# sudo pacman -S python-dasbus
```

---

## 2. Running the Test D-Bus Listener

Run the script from the root of the repository:

```bash
python3 scripts/test_notify_listener.py
```

The script registers a service named `org.rmd.test.Notifications` on the Session Bus and prints received notifications to the console:

```text
Listening on org.rmd.test.Notifications ...
Press Ctrl+C to exit.
```

---

## 3. Configuring `rmd` to Use the Test Service

1. Set the custom D-Bus service name:
   ```bash
   rmd config dbus-service org.rmd.test.Notifications
   ```

2. The `dbus-service` parameter is applied **on the fly**. The daemon automatically uses the new D-Bus service name on the next notification dispatch. No daemon restart is required.

---

## 4. Verification of Notification Delivery

### Regular Reminder

In a separate terminal, schedule a reminder for 5 seconds from now:

```bash
rmd +5s "Custom D-Bus service test"
```

The terminal window running the listener script will output:

```text
=== Notification received ===
App:     rmd
Summary: rmd
Body:    Custom D-Bus service test
Timeout: -1
Hints:   {}
=============================
```

### Checking Missed Reminders Logic

To verify the handling of missed reminders (e.g., if the daemon was offline when a reminder was scheduled to trigger):

1. Run the helper test script:
   ```bash
   scripts/test_missed.sh
   ```
   This script creates an isolated temporary environment with reminders in the past and automatically starts a test instance of the `rmd` daemon.

2. The test daemon will immediately detect the missed reminders, dispatch them to your configured custom D-Bus service, and log them.

3. Once verified, stop the test daemon by pressing `Ctrl+C` in that terminal.

---

## 5. Resetting D-Bus Service Configuration

Once testing is finished, restore the default notification service name (`org.freedesktop.Notifications`):

```bash
rmd config dbus-service org.freedesktop.Notifications
```
