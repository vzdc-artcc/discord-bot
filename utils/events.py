def format_event_time_range(start_str: str, end_str: str) -> str:
    try:
        start_dt = date_parser.parse(start_str)
        end_dt = date_parser.parse(end_str)

        date_format = "%B %d"
        if start_dt.year != datetime.utcnow().year:
            date_format += ", %Y"

        start_date_formatted = start_dt.strftime(date_format)
        end_date_formatted = end_dt.strftime(date_format)

        time_format = "%H%Mz"

        if start_dt.date() == end_dt.date():
            return f"{start_date_formatted} | {start_dt.strftime(time_format)} - {end_dt.strftime(time_format)}"
        else:
            return (
                f"{start_date_formatted} {start_dt.strftime(time_format)} - "
                f"{end_date_formatted} {end_dt.strftime(time_format)}"
            )

    except Exception as e:
        print(f"Error parsing event times {start_str} - {end_str}: {e}")
        return f"{start_str} - {end_str}"

def get_banner_url(banner_key: str) -> str:
    if not IMAGE_BASE_URL:
        print("Warning: IMAGE_BASE_URL not configured. Cannot construct banner URL.")
        return None
    return f"{IMAGE_BASE_URL}{banner_key}"