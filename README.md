# marimow

> [!NOTE]
> Only works on Linux; signal handling on macOS is different and I don't have a
> Mac. You can open a pull request with a fix; see
> [`src/lib.rs:run_marimo`](https://github.com/adityasz/marimow/tree/master/src/lib.rs).

* Convert a python file with cells separated by `# %%` into [marimo](marimo)'s format

  ```python
  marimow convert notebook.py output.py
  ```

  marimo handles data dependencies automatically when the output is opened in
  marimo (see [marimo docs](https://docs.marimo.io/guides/editor_features/watching/#using-your-own-editor)),
  so it is not necessary to add them to function signatures.

* Edit a python file with cells separated by `# %%` in your favourite editor
  (vim) with your favourite type checker, and marimow will convert it to
  [marimo](https://github.com/marimo-team/marimo)'s format on every write, so
  that marimo can live reload it in the browser frontend.

  `marimow edit [OPTIONS] path/to/notebook.py` is just `marimo edit --watch
  [OPTIONS] .marimow_cache/path/to/notebook.py`; all that marimow does is sync
  `.marimow_cache/path/to/notebook.py` with `path/to/notebook.py` for marimo to
  watch.

> [!TIP]
> marimo can [autorun cells](https://docs.marimo.io/guides/editor_features/watching/#watching-for-changes-to-your-notebook).

## Format

* The first cell is a setup cell:

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

* If you don't want a setup cell, keep the first cell blank:

  ```python
  # Each `# %%` starts a new cell.
  #
  # Cells only containing whitespaces and comments are ignored (there are better
  # ways to add text to a marimo notebook than to add a cell with comments).
  #
  # In other cells, comments are preserved.
  # %%

  x = np.array([1, 2, 3])
  ```

  gets converted to

  ```python
  import marimo

  app = marimo.App()


  @app.cell
  def _():
      x = np.array([1, 2, 3])


  if __name__ == "__main__":
      app.run()
  ```


## Config file

You can change the cache directory by adding `cache_dir = <PATH>` to the config
file in `${XDG_CONFIG_HOME:-$HOME/.config/marimow}/marimow/config.toml`.

> [!NOTE]
> Note that if the cache directory is set to
> `${XDG_CACHE_HOME:-$HOME/.cache}/marimow`, marimo does not autorun cells.
> (This may be a bug in marimo.) Thus, the default is `.marimow_cache` (in the
> directory where marimow is executed).
