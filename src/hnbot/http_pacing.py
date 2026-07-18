import asyncio
import time
from collections.abc import Awaitable
from collections.abc import Callable
from math import isfinite


class RequestPacer:
    """Space request starts and allow server-directed global cooldowns."""

    def __init__(
        self,
        min_interval_seconds: float,
        *,
        clock: Callable[[], float] = time.monotonic,
        sleep: Callable[[float], Awaitable[None]] = asyncio.sleep,
    ) -> None:
        if not isfinite(min_interval_seconds) or min_interval_seconds < 0:
            raise ValueError("min_interval_seconds must be finite and non-negative")

        self._min_interval_seconds = min_interval_seconds
        self._clock = clock
        self._sleep = sleep
        self._next_request_at = 0.0
        self._lock = asyncio.Lock()

    async def wait(self) -> None:
        while True:
            async with self._lock:
                now = self._clock()
                delay = self._next_request_at - now
                if delay <= 0:
                    self._next_request_at = now + self._min_interval_seconds
                    return

            await self._sleep(delay)

    async def defer(self, seconds: float) -> None:
        if not isfinite(seconds) or seconds < 0:
            raise ValueError("seconds must be finite and non-negative")

        async with self._lock:
            self._next_request_at = max(self._next_request_at, self._clock() + seconds)
