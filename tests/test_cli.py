from types import SimpleNamespace

import pytest
from typer.testing import CliRunner

import hnbot.cli as cli_module


class FakeCommandApp:
    def __init__(self) -> None:
        self.settings = SimpleNamespace(feed_poll_interval_seconds=30.0)
        self.create_calls = 0
        self.serve_intervals: list[float] = []

    def serve(self, poll_interval_seconds: float) -> None:
        self.serve_intervals.append(poll_interval_seconds)


@pytest.fixture
def fake_command_app(monkeypatch) -> FakeCommandApp:
    command_app = FakeCommandApp()

    def fake_create_app() -> FakeCommandApp:
        command_app.create_calls += 1
        return command_app

    monkeypatch.setattr(cli_module, "_create_app", fake_create_app)
    return command_app


def test_bare_command_shows_help_without_creating_runtime_app(fake_command_app: FakeCommandApp) -> None:
    result = CliRunner().invoke(cli_module.app, [])

    assert result.exit_code == 2
    assert "serve" in result.output
    assert fake_command_app.create_calls == 0
    assert fake_command_app.serve_intervals == []


def test_main_command_is_rejected_without_creating_runtime_app(fake_command_app: FakeCommandApp) -> None:
    result = CliRunner().invoke(cli_module.app, ["main"])

    assert result.exit_code == 2
    assert "No such command 'main'" in result.output
    assert fake_command_app.create_calls == 0
    assert fake_command_app.serve_intervals == []


def test_serve_uses_configured_poll_interval(fake_command_app: FakeCommandApp) -> None:
    result = CliRunner().invoke(cli_module.app, ["serve"])

    assert result.exit_code == 0
    assert fake_command_app.create_calls == 1
    assert fake_command_app.serve_intervals == [30.0]


def test_serve_cli_poll_interval_overrides_configuration(fake_command_app: FakeCommandApp) -> None:
    result = CliRunner().invoke(cli_module.app, ["serve", "--poll-interval", "5"])

    assert result.exit_code == 0
    assert fake_command_app.serve_intervals == [5.0]


def test_serve_rejects_poll_interval_below_one_second(fake_command_app: FakeCommandApp) -> None:
    result = CliRunner().invoke(cli_module.app, ["serve", "--poll-interval", "0.5"])

    assert result.exit_code == 2
    assert fake_command_app.serve_intervals == []


def test_help_lists_only_service_command() -> None:
    result = CliRunner().invoke(cli_module.app, ["--help"])

    assert result.exit_code == 0
    assert "main" not in result.stdout
    assert "serve" in result.stdout
