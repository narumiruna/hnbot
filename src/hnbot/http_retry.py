from collections.abc import Awaitable
from collections.abc import Callable
from datetime import UTC
from datetime import datetime
from email.utils import parsedate_to_datetime
from typing import ParamSpec
from typing import TypeVar
from typing import cast

import httpx
from loguru import logger
from tenacity import RetryCallState
from tenacity import retry
from tenacity import retry_if_exception
from tenacity import stop_after_attempt
from tenacity import wait_exponential_jitter

HTTP_RETRY_ATTEMPTS = 3
_DEFAULT_WAIT = wait_exponential_jitter(initial=1, max=8)

_P = ParamSpec("_P")
_T = TypeVar("_T")


def is_transient_http_error(exc: BaseException) -> bool:
    if isinstance(exc, httpx.HTTPStatusError):
        status_code = exc.response.status_code
        return status_code == 429 or 500 <= status_code < 600

    return isinstance(exc, httpx.RequestError)


def retry_after_seconds(exc: BaseException) -> float | None:
    if not isinstance(exc, httpx.HTTPStatusError):
        return None

    retry_after = exc.response.headers.get("Retry-After")
    if retry_after is None:
        return None

    try:
        return max(float(retry_after), 0.0)
    except ValueError:
        try:
            parsed_dt = parsedate_to_datetime(retry_after)
        except (TypeError, ValueError, IndexError, OverflowError):
            return None

    if parsed_dt.tzinfo is None:
        parsed_dt = parsed_dt.replace(tzinfo=UTC)
    return max((parsed_dt - datetime.now(UTC)).total_seconds(), 0.0)


def _retry_exception(retry_state: RetryCallState) -> BaseException | None:
    if retry_state.outcome is None:
        return None
    return retry_state.outcome.exception()


def wait_for_transient_http_error(retry_state: RetryCallState) -> float:
    exc = _retry_exception(retry_state)
    if exc is None:
        return _DEFAULT_WAIT(retry_state)

    retry_after = retry_after_seconds(exc)
    if retry_after is not None:
        return retry_after

    return _DEFAULT_WAIT(retry_state)


def log_transient_http_retry(retry_state: RetryCallState, *, subject: str = "HTTP request") -> None:
    exc = _retry_exception(retry_state)
    if exc is None:
        return

    logger.warning(
        "Transient HTTP error for {} on attempt {}: {}",
        subject,
        retry_state.attempt_number,
        exc,
    )


def retry_transient_http_errors(
    *,
    before_sleep: Callable[[RetryCallState], None] | None = None,
) -> Callable[[Callable[_P, Awaitable[_T]]], Callable[_P, Awaitable[_T]]]:
    retry_decorator = retry(
        stop=stop_after_attempt(HTTP_RETRY_ATTEMPTS),
        wait=wait_for_transient_http_error,
        retry=retry_if_exception(is_transient_http_error),
        before_sleep=before_sleep or log_transient_http_retry,
        reraise=True,
    )
    return cast(Callable[[Callable[_P, Awaitable[_T]]], Callable[_P, Awaitable[_T]]], retry_decorator)
