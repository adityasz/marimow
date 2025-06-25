import marimo

app = marimo.App()


@app.cell
def _():
    xs = [1, 2, 3]
    return (xs,)


if __name__ == "__main__":
    app.run()
