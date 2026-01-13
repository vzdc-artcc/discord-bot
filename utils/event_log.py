# ...existing code...
import json
import os
import tempfile
import fcntl
import typing as t
import re
from datetime import datetime, timezone


def _log_filepath(guild_id: t.Optional[int]) -> str:
    """Return the filepath for the guild-specific or global event posting log."""
    base_dir = os.path.join(os.path.dirname(__file__), "..", "data")
    # resolve to absolute normalized path
    base_dir = os.path.normpath(os.path.abspath(base_dir))
    if not os.path.isdir(base_dir):
        # create data dir if missing
        try:
            os.makedirs(base_dir, exist_ok=True)
        except Exception:
            pass

    if guild_id:
        return os.path.join(base_dir, f"event_position_postings_guild_{int(guild_id)}.json")
    else:
        return os.path.join(base_dir, "event_position_postings_global.json")


def _normalize_title(title: str) -> str:
    """Normalize an event title for use as a key: lowercase, strip punctuation, collapse whitespace."""
    if not isinstance(title, str):
        return ""
    t = title.strip().lower()
    # remove punctuation except spaces
    t = re.sub(r"[^\w\s-]", "", t)
    # collapse whitespace
    t = re.sub(r"\s+", " ", t)
    return t


def make_event_key(event_id: t.Optional[t.Union[str, int]], event_title: t.Optional[str], guild_id: t.Optional[int]) -> str:
    """Generate a stable key for an event entry. Prefer event_id when present."""
    if event_id:
        return f"id:{str(event_id)}"
    title_part = _normalize_title(event_title or "")
    gid = str(guild_id) if guild_id is not None else "global"
    return f"title:{title_part}::guild:{gid}"


def load_log(guild_id: t.Optional[int]) -> dict:
    """Load the log JSON and return as a dict. Returns empty dict if file missing or invalid."""
    path = _log_filepath(guild_id)
    if not os.path.exists(path):
        return {}
    try:
        with open(path, "r", encoding="utf-8") as fh:
            try:
                # use shared lock while reading
                try:
                    fcntl.flock(fh.fileno(), fcntl.LOCK_SH)
                except Exception:
                    # if flock unavailable, proceed without it
                    pass
                data = json.load(fh)
            finally:
                try:
                    fcntl.flock(fh.fileno(), fcntl.LOCK_UN)
                except Exception:
                    pass
        if isinstance(data, dict):
            return data
    except Exception:
        # Corrupt file or other IO problem -> return empty and caller may overwrite
        return {}
    return {}


def save_log(guild_id: t.Optional[int], data: dict) -> None:
    """Atomically write the log dict to the guild-specific file.

    Writes to a temp file in the same directory and then os.replace to ensure atomicity.
    """
    path = _log_filepath(guild_id)
    ddir = os.path.dirname(path)
    if not os.path.isdir(ddir):
        os.makedirs(ddir, exist_ok=True)

    # Write JSON to a temp file then atomically replace
    tmp_fd, tmp_path = tempfile.mkstemp(prefix=".tmp_event_log_", dir=ddir)
    try:
        with os.fdopen(tmp_fd, "w", encoding="utf-8") as fh:
            # acquire exclusive lock on temp file while writing
            try:
                fcntl.flock(fh.fileno(), fcntl.LOCK_EX)
            except Exception:
                pass
            json.dump(data, fh, indent=2, ensure_ascii=False)
            fh.flush()
            try:
                os.fsync(fh.fileno())
            except Exception:
                pass
            try:
                fcntl.flock(fh.fileno(), fcntl.LOCK_UN)
            except Exception:
                pass
        # replace target
        os.replace(tmp_path, path)
    except Exception:
        # cleanup temp file on failure
        try:
            if os.path.exists(tmp_path):
                os.remove(tmp_path)
        except Exception:
            pass
        raise


def _now_iso() -> str:
    return datetime.now(timezone.utc).isoformat()


# ...existing code...

