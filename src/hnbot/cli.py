import logging

import typer

from hnbot.app import App

logger = logging.getLogger(__name__)


app = typer.Typer()


@app.command()
def main() -> None:
    app = App()
    app.run()
