from __future__ import annotations

import time
from datetime import UTC
from datetime import datetime
from email.utils import parsedate_to_datetime

import feedparser

from hnbot.rss import parse_datetime


def safe_iso(obj: object) -> str:
    if isinstance(obj, datetime):
        return obj.isoformat()
    return str(obj)


def manual_from_struct(struct: tuple) -> object:
    try:
        y, m, d, h, mi, s = struct[:6]
        return datetime(y, m, d, h, mi, s, tzinfo=UTC)
    except (TypeError, ValueError, OverflowError) as e:
        return f"manual conversion error: {e!r}"


def mktime_to_dt(struct: tuple) -> object:
    try:
        ts = time.mktime(struct)  # treats struct as localtime
        return datetime.fromtimestamp(ts, tz=UTC)
    except (TypeError, ValueError, OverflowError, OSError) as e:
        return f"mktime conversion error: {e!r}"


def main() -> None:
    url = "https://hnrss.org/newest?points=100"
    print("Fetching:", url)
    feed = feedparser.parse(url)

    entries = feed.get("entries", [])[:5]
    if not entries:
        print("No entries returned; feed keys:", list(feed.keys()))
        return

    for i, entry in enumerate(entries, start=1):
        print("\n" + "=" * 72)
        print(f"Entry {i}: {entry.get('title')!r}")

        p_str = entry.get("published")
        p_struct = entry.get("published_parsed")

        print("published (raw):", p_str)
        print("published_parsed (repr):", repr(p_struct))

        if p_struct is not None:
            try:
                parsed_dt = parse_datetime(p_struct)
            except (TypeError, ValueError, IndexError) as e:
                parsed_dt = f"parse_datetime raised: {e!r}"

            print("parse_datetime(published_parsed):", safe_iso(parsed_dt))
            print("manual_from_struct:", safe_iso(manual_from_struct(p_struct)))
            print("mktime -> utc:", safe_iso(mktime_to_dt(p_struct)))
        else:
            print("published_parsed is None; skipping struct-based conversions")

        if p_str:
            try:
                dt_parsedate = parsedate_to_datetime(p_str)
            except (TypeError, ValueError, IndexError) as e:
                dt_parsedate = f"parsedate_to_datetime raised: {e!r}"
            print("parsedate_to_datetime(published):", safe_iso(dt_parsedate))

    print("\n" + "=" * 72)
    print("Done")


if __name__ == "__main__":
    main()
