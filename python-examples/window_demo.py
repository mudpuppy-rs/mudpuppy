import logging
import random
import math
import asyncio
from datetime import datetime

import pup
from pup import (
    Session,
    Timer,
    Buffer,
    FloatingWindow,
    Position,
    Size,
    DialogPriority,
    MudLine,
    OutputItem,
)

logger = logging.getLogger(__name__)

# Animation state
global_window_state = {
    "angle": 0.0,
    "packets_sent": 0,
    "packets_recv": 0,
    "connections": [],
    "graph_data": [0] * 20,
}

session_window_state = {}

# Store timers to prevent garbage collection
global_timers = []
session_timers = {}


async def setup_demo(sesh: Session):
    """Setup both global and per-session floating window demos."""
    logger.info(f"Setting up window demo for session: {sesh.character}")

    # Initialize session state
    session_window_state[sesh.id] = {
        "hp": 100,
        "mp": 80,
        "xp": 0,
        "combat_log": [],
    }

    # Create global network monitor window (only once)
    if not hasattr(setup_demo, "_global_setup"):
        setup_demo._global_setup = True
        await setup_global_window()

    # Create per-session stats window
    await setup_session_window(sesh)


async def setup_global_window():
    """Create a global floating window showing fake network activity."""
    logger.info("Creating global network monitor window")

    # Create buffer for global window
    buffer = Buffer("network-monitor")
    buffer.config.border_top = True
    buffer.config.border_bottom = True
    buffer.config.border_left = True
    buffer.config.border_right = True

    # Create the floating window
    window = FloatingWindow(
        buffer=buffer,
        position=Position.percent(60, 5),
        size=Size.percent(35, 25),
        title="Network Monitor"
    )

    # Get global dialog manager and show window
    dm = await pup.dialog_manager()
    dm.show_floating_window(
        id="network-monitor",
        window=window,
        dismissible=False,
        priority=DialogPriority.Low,
        timeout=None
    )

    # Create timer for content updates (2 updates per second)
    # Timer duration is in SECONDS, not milliseconds
    content_timer = Timer("global-window-content", 1, callback=update_global_content)
    content_timer.start()
    global_timers.append(content_timer)  # Store to prevent GC

    logger.info("Global network monitor started")


async def setup_session_window(sesh: Session):
    """Create a per-session floating window showing fake session stats."""
    logger.info(f"Creating session stats window for {sesh.character}")

    # Create buffer for session window
    buffer = Buffer(f"session-stats-{sesh.id}")
    buffer.config.border_top = True
    buffer.config.border_bottom = True
    buffer.config.border_left = True
    buffer.config.border_right = True

    # Create the floating window
    window = FloatingWindow(
        buffer=buffer,
        position=Position.percent(5, 60),
        size=Size.percent(35, 30),
        title=f"{sesh.character} Stats"
    )

    # Get session dialog manager and show window
    dm = await sesh.dialog_manager()
    dm.show_floating_window(
        id=f"session-stats-{sesh.id}",
        window=window,
        dismissible=False,
        priority=DialogPriority.Low,
        timeout=None
    )

    # Create timer for content updates with session attached
    # Timer duration is in SECONDS, not milliseconds
    content_timer = Timer(
        f"session-window-content-{sesh.id}",
        1,
        callback=update_session_content,
        session=sesh
    )
    content_timer.start()
    session_timers[sesh.id] = content_timer  # Store to prevent GC

    logger.info(f"Session stats window started for {sesh.character}")


async def update_global_content(timer: Timer):
    """Update the content of the global network monitor."""
    logger.info(f"update_global_content called! hit_count={timer.hit_count}")
    state = global_window_state

    # Generate fake network activity
    state["packets_sent"] += random.randint(50, 500)
    state["packets_recv"] += random.randint(100, 800)

    # Random connection events
    if random.random() < 0.3:
        ips = ["192.168.1.100", "10.0.0.50", "172.16.0.200", "8.8.8.8"]
        state["connections"].append(random.choice(ips))
        if len(state["connections"]) > 5:
            state["connections"].pop(0)

    # Update graph data
    state["graph_data"].append(random.randint(0, 100))
    if len(state["graph_data"]) > 20:
        state["graph_data"].pop(0)

    # Render content
    dm = await pup.dialog_manager()
    buffer_py = dm.get_floating_window_buffer("network-monitor")

    if buffer_py:
        # Build content
        lines = []
        lines.append(f"  \x1b[1;36mNetwork Activity Monitor\x1b[0m")
        lines.append(f"  {datetime.now().strftime('%H:%M:%S')}")
        lines.append("")
        lines.append(f"  \x1b[32m↑\x1b[0m Sent:     {state['packets_sent']:>8} packets")
        lines.append(f"  \x1b[33m↓\x1b[0m Received: {state['packets_recv']:>8} packets")
        lines.append("")
        lines.append("  \x1b[1mActivity Graph:\x1b[0m")

        # ASCII graph
        max_val = max(state["graph_data"]) if state["graph_data"] else 1
        for i in range(5, -1, -1):
            threshold = (i / 5.0) * max_val
            line = "  "
            for val in state["graph_data"]:
                if val >= threshold:
                    line += "\x1b[32m█\x1b[0m"
                else:
                    line += " "
            lines.append(line)
        lines.append("  " + "─" * 20)

        if state["connections"]:
            lines.append("")
            lines.append("  \x1b[1mRecent Connections:\x1b[0m")
            for conn in state["connections"][-3:]:
                lines.append(f"  \x1b[36m•\x1b[0m {conn}")

        # Update buffer with new content
        for line in lines:
            line_bytes = (line + "\n").encode("utf-8")
            buffer_py.add(OutputItem.mud(MudLine(line_bytes)))


async def update_session_content(timer: Timer):
    """Update the content of the session stats window."""
    logger.info(f"update_session_content called! hit_count={timer.hit_count}")
    sesh = timer.session
    if not sesh or sesh.id not in session_window_state:
        logger.warning(f"No session found or session not in state")
        return

    state = session_window_state[sesh.id]

    # Simulate stat changes
    state["hp"] = max(0, min(100, state["hp"] + random.randint(-5, 10)))
    state["mp"] = max(0, min(100, state["mp"] + random.randint(-8, 12)))
    state["xp"] += random.randint(0, 50)

    # Add fake combat events
    events = [
        "You hit the goblin for 15 damage!",
        "The goblin misses you.",
        "You cast lightning bolt!",
        "Critical hit! 30 damage!",
        "You gained 25 experience.",
        "The goblin dies.",
        "You found 10 gold!",
    ]

    if random.random() < 0.4:
        state["combat_log"].append(random.choice(events))
        if len(state["combat_log"]) > 6:
            state["combat_log"].pop(0)

    # Get session dialog manager and update buffer
    dm = await sesh.dialog_manager()
    buffer_py = dm.get_floating_window_buffer(f"session-stats-{sesh.id}")

    if buffer_py:
        # Build content
        lines = []
        lines.append(f"  \x1b[1;35m{sesh.character} Status\x1b[0m")
        lines.append(f"  {datetime.now().strftime('%H:%M:%S')}")
        lines.append("")

        # HP bar
        hp_pct = state["hp"] / 100.0
        hp_bar = _make_bar(hp_pct, 20, "\x1b[31m", "\x1b[90m")
        lines.append(f"  \x1b[1mHP:\x1b[0m [{hp_bar}\x1b[0m] {state['hp']:>3}/100")

        # MP bar
        mp_pct = state["mp"] / 100.0
        mp_bar = _make_bar(mp_pct, 20, "\x1b[34m", "\x1b[90m")
        lines.append(f"  \x1b[1mMP:\x1b[0m [{mp_bar}\x1b[0m] {state['mp']:>3}/100")

        lines.append("")
        lines.append(f"  \x1b[1mXP:\x1b[0m {state['xp']:>6}")

        if state["combat_log"]:
            lines.append("")
            lines.append("  \x1b[1mCombat Log:\x1b[0m")
            for event in state["combat_log"][-5:]:
                lines.append(f"  \x1b[90m>\x1b[0m {event}")

        # Update buffer with new content
        for line in lines:
            line_bytes = (line + "\n").encode("utf-8")
            buffer_py.add(OutputItem.mud(MudLine(line_bytes)))


def _make_bar(percentage, width, filled_color, empty_color):
    """Create an ASCII progress bar."""
    filled = int(width * percentage)
    empty = width - filled
    return f"{filled_color}{'█' * filled}{empty_color}{'░' * empty}"


logger.info("window_demo module loaded")
