import logging
from typing import Annotated

import typer
from dotenv import find_dotenv
from dotenv import load_dotenv

from hnbot.app import App
from hnbot.settings import get_settings
from hnbot.utils import configure_logfire

logger = logging.getLogger(__name__)


app = typer.Typer(invoke_without_command=True, no_args_is_help=False)


def _create_app() -> App:
    load_dotenv(
        find_dotenv(),
        override=True,
    )

    settings = get_settings()

    configure_logfire(settings)

    return App(settings)


@app.callback(invoke_without_command=True)
def cli(ctx: typer.Context) -> None:
    """Run one batch by default, or select a long-running command."""
    if ctx.invoked_subcommand is None:
        _create_app().run()


@app.command()
def main() -> None:
    """Process one feed batch and exit."""
    _create_app().run()


@app.command()
def serve(
    poll_interval: Annotated[
        float | None,
        typer.Option(
            "--poll-interval",
            min=1.0,
            help="Seconds to wait between completed feed batches; overrides configuration.",
        ),
    ] = None,
) -> None:
    """Continuously poll the feed and process unseen entries."""
    runtime_app = _create_app()
    interval = runtime_app.settings.feed_poll_interval_seconds if poll_interval is None else poll_interval
    runtime_app.serve(interval)
