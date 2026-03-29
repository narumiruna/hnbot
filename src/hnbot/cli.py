import logging

import logfire
import typer
from dotenv import find_dotenv
from dotenv import load_dotenv

from hnbot.app import App
from hnbot.settings import get_settings
from hnbot.utils import configure_logfire

logger = logging.getLogger(__name__)


app = typer.Typer()


@app.command()
def main() -> None:
    with logfire.span("hnbot.cli.main"):
        load_dotenv(
            find_dotenv(),
            override=True,
        )

        settings = get_settings()
        configure_logfire(settings)

        app = App(settings)
        app.run()
