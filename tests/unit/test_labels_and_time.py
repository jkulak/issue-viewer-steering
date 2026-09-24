from datetime import UTC, datetime, timedelta
from zoneinfo import ZoneInfo

from issue_viewer import labels, timefmt


def test_known_labels_keep_the_usual_colours_and_others_get_a_stable_one():
    assert labels.color("bug") == "d73a4a"
    assert labels.color("Enhancement") == "a2eeef"
    assert labels.color("wiki-api") == labels.color("wiki-api")
    assert labels.color("wiki-api") in labels.PALETTE


def test_label_text_is_dark_on_light_colours_and_white_on_dark_ones():
    assert labels.text_color("a2eeef") == "#1f2328"
    assert labels.text_color("0052cc") == "#ffffff"
    assert labels.style("bug") == "background-color:#d73a4a;color:#ffffff"


def test_relative_times_read_like_github():
    now = datetime(2026, 9, 24, 12, 0, tzinfo=UTC)
    assert timefmt.relative(now - timedelta(seconds=10), now) == "just now"
    assert timefmt.relative(now - timedelta(minutes=5), now) == "5 minutes ago"
    assert timefmt.relative(now - timedelta(hours=3), now) == "3 hours ago"
    assert timefmt.relative(now - timedelta(days=1), now) == "yesterday"
    assert timefmt.relative(now - timedelta(days=4), now) == "4 days ago"
    assert timefmt.relative(datetime(2026, 3, 2, tzinfo=UTC), now) == "on Mar 2"
    assert timefmt.relative(datetime(2025, 3, 2, tzinfo=UTC), now) == "on Mar 2, 2025"


def test_absolute_times_are_shown_in_the_configured_zone():
    moment = datetime(2026, 9, 24, 12, 0, tzinfo=UTC)
    assert timefmt.absolute(moment, ZoneInfo("Europe/Warsaw")) == "Sep 24, 2026, 14:00 CEST"
