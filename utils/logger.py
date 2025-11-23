import datetime as dt
import json
import logging
try:
    from typing import override
except Exception:
    from typing_extensions import override

LOG_RECORD_BUILTIN_ATTRS = {
    "args",
    "asctime",
    "created",
    "exc_info",
    "exc_text",
    "filename",
    "funcName",
    "levelname",
    "levelno",
    "lineno",
    "module",
    "msecs",
    "message",
    "msg",
    "name",
    "pathname",
    "process",
    "processName",
    "relativeCreated",
    "stack_info",
    "thread",
    "threadName",
    "taskName",
}


class MyJSONFormatter(logging.Formatter):
    def __init__(
        self,
        *,
        fmt_keys: dict[str, str] | None = None,
    ):
        super().__init__()
        self.fmt_keys = fmt_keys if fmt_keys is not None else {}

    @override
    def format(self, record: logging.LogRecord) -> str:
        message = self._prepare_log_dict(record)
        return json.dumps(message, default=str)

    def _prepare_log_dict(self, record: logging.LogRecord):
        always_fields = {
            "message": record.getMessage(),
            "timestamp": dt.datetime.fromtimestamp(
                record.created, tz=dt.timezone.utc
            ).isoformat(),
        }
        # Be defensive: exc_info may be malformed (e.g., a boolean) when handlers
        # or other code erroneously set it. Only attempt to format if it looks
        # like the expected exc_info tuple or an Exception instance. If formatting
        # fails, capture a simple string representation instead.
        if record.exc_info:
            try:
                if isinstance(record.exc_info, tuple) or isinstance(record.exc_info, BaseException):
                    always_fields["exc_info"] = self.formatException(record.exc_info)
                else:
                    # Unexpected type; store repr to avoid raising in formatter
                    always_fields["exc_info"] = repr(record.exc_info)
            except Exception:
                try:
                    always_fields["exc_info"] = str(record.exc_info)
                except Exception:
                    always_fields["exc_info"] = "<unformattable exc_info>"

        if record.stack_info:
            try:
                always_fields["stack_info"] = self.formatStack(record.stack_info)
            except Exception:
                try:
                    always_fields["stack_info"] = str(record.stack_info)
                except Exception:
                    always_fields["stack_info"] = "<unformattable stack_info>"

        message = {
            key: msg_val
            if (msg_val := always_fields.pop(val, None)) is not None
            else getattr(record, val)
            for key, val in self.fmt_keys.items()
        }
        message.update(always_fields)

        for key, val in record.__dict__.items():
            if key not in LOG_RECORD_BUILTIN_ATTRS:
                message[key] = val

        return message


class NonErrorFilter(logging.Filter):
    @override
    def filter(self, record: logging.LogRecord) -> bool | logging.LogRecord:
        return record.levelno <= logging.INFO