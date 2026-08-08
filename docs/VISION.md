# Product Vision & Philosophy

`rmd` is not a task manager. It is a zero-friction, hard-interrupt system for Linux.

## Core Principles

### 1. Protection of the Flow State
Human focus is fragile. Daily task management should rely on **pull-based systems** (To-Do lists, Kanban boards, plain-text notebooks) that you consult on your own terms when you are ready to switch context.

Reminders in `rmd` are **push-based interrupts**. They exist solely for urgent, time-bound events that will cause immediate negative consequences if missed right now:
- Food burning on the stove while you are coding.
- Joining a hard-scheduled meeting.
- Taking medicine at a precise hour.

Because an `rmd` notification disrupts your flow state, it uses critical system urgency. It should be used sparingly.

### 2. Resistance to List Rot
Task managers often fail over long periods because tasks are added faster than they are completed. Unfinished items accumulate, leading to notification fatigue and system abandonment.

`rmd` avoids list rot through three constraints:
- **One-shot execution:** Reminders trigger once, deliver their payload, and get out of the way.
- **Zero taxonomy:** No tags, no projects, no priorities, no recurring complex schedules.
- **System-agnostic longevity:** Setting a reminder for an event months or a year in the future (e.g., domain or server renewal) requires zero ongoing usage of a specific productivity app—just your running Linux system.

### 3. Architectural Consequences
- **Minimal memory footprint:** Must run permanently in the background without draining system resources (~5 MiB RSS).
- **Fast interaction:** Setting a reminder must take less than 2 seconds from terminal invocation.
- **Resilience over restarts:** State must survive reboot/sleep cycles without losing missed alerts.
