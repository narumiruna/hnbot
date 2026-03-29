import logging

import typer
from dotenv import find_dotenv
from dotenv import load_dotenv

from hnbot.app import App
from hnbot.utils import configure_logfire

logger = logging.getLogger(__name__)


app = typer.Typer()


@app.command()
def main() -> None:
    load_dotenv(
        find_dotenv(),
        override=True,
    )

    configure_logfire()

    app = App()
    app.run()
