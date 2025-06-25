import marimo

app = marimo.App()

with app.setup:
    import marimo as mo
    import matplotlib.pyplot as plt


@app.cell
def _():
    xs = [1, 2, 3, 4] # %%


@app.cell
def _():
    print(xs)


if __name__ == "__main__":
    app.run()
