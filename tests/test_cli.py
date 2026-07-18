from types import SimpleNamespace

import pytest
from typer.testing import CliRunner

import hnbot.cli as cli_module


class FakeCommandApp:
    def __init__(self) -> None:
        self.settings = SimpleNamespace(feed_poll_interval_seconds=30.0)
        self.run_calls = 0
        self.serve_intervals: list[float] = []

    def run(self) -> None:
        self.run_calls += 1

    def serve(self, poll_interval_seconds: float) -> None:
        self.serve_intervals.append(poll_interval_seconds)


@pytest.fixture
def fake_command_app(monkeypatch) -> FakeCommandApp:
    command_app = FakeCommandApp()
    monkeypatch.setattr(cli_module, "get_settings", lambda: command_app.settings)
    monkeypatch.setattr(cli_module, "configure_logfire", lambda _settings: None)
    monkeypatch.setattr(cli_module, "App", lambda _settings: command_app)
    monkeypatch.setattr(cli_module, "_create_app", lambda: command_app, raising=False)
    return command_app


def test_bare_command_runs_one_batch(fake_command_app: FakeCommandApp) -> None:
    result = CliRunner().invoke(cli_module.app, [])

    assert result.exit_code == 0
    assert fake_command_app.run_calls == 1
    assert fake_command_app.serve_intervals == []


def test_main_command_runs_one_batch(fake_command_app: FakeCommandApp) -> None:
    result = CliRunner().invoke(cli_module.app, ["main"])

    assert result.exit_code == 0
    assert fake_command_app.run_calls == 1
    assert fake_command_app.serve_intervals == []


def test_serve_uses_configured_poll_interval(fake_command_app: FakeCommandApp) -> None:
    result = CliRunner().invoke(cli_module.app, ["serve"])

    assert result.exit_code == 0
    assert fake_command_app.run_calls == 0
    assert fake_command_app.serve_intervals == [30.0]


def test_serve_cli_poll_interval_overrides_configuration(fake_command_app: FakeCommandApp) -> None:
    result = CliRunner().invoke(cli_module.app, ["serve", "--poll-interval", "5"])

    assert result.exit_code == 0
    assert fake_command_app.serve_intervals == [5.0]


def test_serve_rejects_poll_interval_below_one_second(fake_command_app: FakeCommandApp) -> None:
    result = CliRunner().invoke(cli_module.app, ["serve", "--poll-interval", "0.5"])

    assert result.exit_code == 2
    assert fake_command_app.serve_intervals == []


def test_help_lists_batch_and_service_commands() -> None:
    result = CliRunner().invoke(cli_module.app, ["--help"])

    assert result.exit_code == 0
    assert "main" in result.stdout
    assert "serve" in result.stdout
