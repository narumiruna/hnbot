import time
from datetime import UTC
from datetime import datetime

from hnbot.rss import _parse_feed
from hnbot.rss import parse_datetime

SAMPLE_RSS = b"""<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Test Feed</title>
    <item>
      <title>First Post</title>
      <link>https://example.com/first</link>
      <comments>https://news.ycombinator.com/item?id=100</comments>
      <pubDate>Sat, 01 Jan 2022 12:00:00 GMT</pubDate>
    </item>
    <item>
      <title>Second Post</title>
      <link>https://example.com/second</link>
      <comments>https://news.ycombinator.com/item?id=200</comments>
      <pubDate>Sat, 02 Jan 2022 12:00:00 GMT</pubDate>
    </item>
  </channel>
</rss>
"""


def test_parse_datetime_uses_only_calendar_fields() -> None:
    published_parsed = time.struct_time((2026, 4, 10, 16, 22, 26, 4, 100, 0))

    assert parse_datetime(published_parsed) == datetime(2026, 4, 10, 16, 22, 26, tzinfo=UTC)


def test_parse_feed_entries_are_reversed() -> None:
    feed = _parse_feed(SAMPLE_RSS)
    # reverse() makes the last XML item (id=200) first and the first XML item (id=100) last
    assert feed.entries[0].id == "200"
    assert feed.entries[1].id == "100"


def test_parse_feed_entry_fields() -> None:
    feed = _parse_feed(SAMPLE_RSS)
    entry = feed.entries[1]  # id=100 (reversed)
    assert entry.title == "First Post"
    assert entry.link == "https://example.com/first"
    assert entry.comment_url == "https://news.ycombinator.com/item?id=100"
    assert entry.id == "100"
    assert isinstance(entry.published_at, datetime)
    assert entry.published_at.tzinfo == UTC
