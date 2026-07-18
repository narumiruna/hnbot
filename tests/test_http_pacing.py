import asyncio

import pytest

from hnbot.http_pacing import RequestPacer


class FakeClock:
    def __init__(self) -> None:
        self.now = 0.0
        self.sleeps: list[float] = []

    def __call__(self) -> float:
        return self.now

    async def sleep(self, seconds: float) -> None:
        self.sleeps.append(seconds)
        self.now += seconds


@pytest.mark.anyio
async def test_request_pacer_spaces_sequential_requests() -> None:
    clock = FakeClock()
    pacer = RequestPacer(2.0, clock=clock, sleep=clock.sleep)

    await pacer.wait()
    first_request_at = clock.now
    await pacer.wait()
    second_request_at = clock.now

    assert first_request_at == 0.0
    assert second_request_at == 2.0
    assert clock.sleeps == [2.0]


@pytest.mark.anyio
async def test_request_pacer_spaces_concurrent_requests() -> None:
    clock = FakeClock()
    pacer = RequestPacer(2.0, clock=clock, sleep=clock.sleep)
    request_times: list[float] = []

    async def make_request() -> None:
        await pacer.wait()
        request_times.append(clock.now)

    await asyncio.gather(make_request(), make_request(), make_request())

    assert request_times == [0.0, 2.0, 4.0]


@pytest.mark.anyio
async def test_request_pacer_defer_extends_existing_interval() -> None:
    clock = FakeClock()
    pacer = RequestPacer(2.0, clock=clock, sleep=clock.sleep)

    await pacer.wait()
    await pacer.defer(30.0)
    await pacer.wait()

    assert clock.now == 30.0
    assert clock.sleeps == [30.0]


@pytest.mark.anyio
async def test_request_pacer_rejects_invalid_durations() -> None:
    for invalid_duration in (-1.0, float("nan"), float("inf")):
        with pytest.raises(ValueError):
            RequestPacer(invalid_duration)

    pacer = RequestPacer(0.0)
    for invalid_duration in (-1.0, float("nan"), float("inf")):
        with pytest.raises(ValueError):
            await pacer.defer(invalid_duration)
