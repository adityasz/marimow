import marimo

app = marimo.App()

with app.setup:
    import numpy as np

    x = np.array([1, 2, 3])
    print(x)

if __name__ == "__main__":
    app.run()
