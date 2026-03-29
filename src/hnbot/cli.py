import logging

import typer
from dotenv import find_dotenv
from dotenv import load_dotenv

from hnbot.app import App

logger = logging.getLogger(__name__)


app = typer.Typer()


@app.command()
def main() -> None:
    load_dotenv(
        find_dotenv(),
        override=True,
    )

    app = App()
    app.run()
