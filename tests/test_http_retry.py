import httpx

from hnbot.http_retry import is_transient_http_error
from hnbot.http_retry import retry_after_seconds


def _http_status_error(status: int, retry_after: str | None = None) -> httpx.HTTPStatusError:
    request = httpx.Request("GET", "https://example.com")
    headers = {"Retry-After": retry_after} if retry_after is not None else None
    response = httpx.Response(status, request=request, headers=headers)
    return httpx.HTTPStatusError("status error", request=request, response=response)


def test_is_transient_http_error_matches_request_errors_429_and_5xx() -> None:
    request = httpx.Request("GET", "https://example.com")

    assert is_transient_http_error(httpx.ConnectTimeout("connect timed out", request=request)) is True
    assert is_transient_http_error(_http_status_error(429)) is True
    assert is_transient_http_error(_http_status_error(503)) is True
    assert is_transient_http_error(_http_status_error(404)) is False
    assert is_transient_http_error(ValueError("not http")) is False


def test_retry_after_seconds_parses_numeric_header() -> None:
    assert retry_after_seconds(_http_status_error(429, retry_after="2.5")) == 2.5


def test_retry_after_seconds_ignores_invalid_header() -> None:
    assert retry_after_seconds(_http_status_error(429, retry_after="not a date")) is None
