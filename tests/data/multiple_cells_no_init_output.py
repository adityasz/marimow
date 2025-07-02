import marimo

app = marimo.App()


@app.cell
def _():
    a = 1


@app.cell
def _():
    b = 2
    c = 3 # %%
    # comments are preserved
    d = 4


@app.cell
def _():
    print(a + b + c + d)


if __name__ == "__main__":
    app.run()
