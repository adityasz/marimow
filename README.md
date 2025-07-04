# marimoW

![CI](https://github.com/adityasz/marimow/actions/workflows/ci.yml/badge.svg)

> [!NOTE]
>
> Only works on Linux; signal handling on macOS is different and I don't have a
> Mac. You can open a pull request with a fix: See
> [`src/lib.rs:run_marimo`](https://github.com/adityasz/marimow/tree/master/src/lib.rs).

A [marimo](https://github.com/marimo-team/marimo) Wrapper.

Because

```python
# %%

x = 1

# %%

print(x)
```

is easier to type than

```python
app.cell()
def _():
    x = 1


app.cell()
def _():
    print(x)
```

- Convert Python scripts having cell delimiters (e.g., `# %%`; see
  [config](#config)) into [marimo's format](https://docs.marimo.io/guides/editor_features/watching/#marimos-file-format):

  ```console
  $ marimow convert notebook.py output.py
  ```

- Edit a python file in any editor and marimoW will convert it to marimo's
  format on every write, so that it can live reload it in the browser frontend.

  ```console
  $ marimow edit [OPTIONS] notebook.py
  ```

  This is equivalent to `marimo edit --watch [OPTIONS] .marimow_cache/notebook.py`,
  where `.marimow_cache/notebook.py` is in marimo's format.

> [!TIP]
> marimo can [autorun cells](https://docs.marimo.io/guides/editor_features/watching/#watching-for-changes-to-your-notebook).


> [!NOTE]
>
> marimo handles data dependencies automatically when the output is opened in
> marimo (see [marimo docs](https://docs.marimo.io/guides/editor_features/watching/#using-your-own-editor)),
> so it is not necessary to add them to function signatures or `return`
> statements.
>
> To really hardcode them to the notebook, just open the output in marimo's web
> UI and save it there.

## Installation

```console
$ cargo install --git https://github.com/adityasz/marimow
```

## Format

- The cell delimiter can be configured in the [config file](#config).

- The first cell is the [setup cell](https://docs.marimo.io/guides/reusing_functions/?h=setup#1-create-a-setup-cell):

  ```python
  import numpy as np

  # %%

  x = np.array([1, 2, 3])
  ```

  gets converted to

  ```python
  import marimo

  app = marimo.App()

  with app.setup:
      import numpy as np


  @app.cell
  def _():
      x = np.array([1, 2, 3])


  if __name__ == "__main__":
      app.run()
  ```

- If you don't want a setup cell, keep the first cell blank:

  ```python
  # Each `# %%` starts a new cell.
  #
  # Cells only containing whitespaces and comments are ignored (there are better
  # ways to add text to a marimo notebook than to add a cell with comments).
  # %%

  x = np.array([1, 2, 3])  # comments are preserved

  # %% everything after the cell delimiter is ignored

  print(x)
  ```

  gets converted to

  ```python
  import marimo

  app = marimo.App()


  @app.cell
  def _():
      x = np.array([1, 2, 3])  # comments are preserved


  @app.cell
  def _():
      print(x)


  if __name__ == "__main__":
      app.run()
  ```

> [!NOTE]
>
> marimoW indents everything in a cell by 4 spaces to put it in the body of
> `@app.cell def _():`. This does **not** affect multiline strings: The
> `@app.cell` decorator probably handles this correctly as the string length
> remains unchanged.
>
> A multiline string having the delimiter in the beginning of a line will cause
> issues, but the delimiter is configurable, so it is up to the user. Zed
> preview 0.193.3's REPL also does not take care of this (and their delimiter
> can't be changed), despite having a parse tree of the file (for syntax
> highlighting).

## Config

marimoW loads its config from
`${XDG_CONFIG_HOME:-$HOME/.config/marimow}/marimow/config.toml`.

The default config is

```toml
cache_dir = ".marimow_cache"
cell_delimiter = "# %%"
```

> [!NOTE]
>
> Note that if the cache directory is set to
> `${XDG_CACHE_HOME:-$HOME/.cache}/marimow`, marimo does not autorun cells.
> (This may be a bug in marimo.) Thus, the default is `.marimow_cache` in the
> directory where marimoW is executed (which unfortunately means one more
> `.gitignore` entry).
