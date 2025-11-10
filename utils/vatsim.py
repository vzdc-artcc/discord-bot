from datetime import datetime


def parse_vatsim_logon_time(logon_time_str: str) -> datetime:

    parts = logon_time_str.replace('Z', '').split('.')
    main_part = parts[0]

    if len(parts) > 1:
        fractional_seconds = parts[1]
        truncated_fractional = fractional_seconds[:6]
        logon_time_str_corrected = f"{main_part}.{truncated_fractional}+00:00"
    else:
        logon_time_str_corrected = f"{main_part}+00:00"

    return datetime.fromisoformat(logon_time_str_corrected)