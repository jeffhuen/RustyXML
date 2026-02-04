defmodule RustyXML.SimpleForm do
  @moduledoc """
  Parse XML into a simple tree of nested tuples.

  Drop-in replacement for `Saxy.SimpleForm`.

  The simple form represents XML as:

      {tag_name, attributes, children}

  Where:
    * `tag_name` is a string
    * `attributes` is a list of `{name, value}` tuples
    * `children` is a list of child elements or text strings

  ## Examples

      RustyXML.SimpleForm.parse_string("<root><item id=\\"1\\">text</item></root>")
      #=> {:ok, {"root", [], [{"item", [{"id", "1"}], ["text"]}]}}

  """

  @type element :: {String.t(), [{String.t(), String.t()}], [element() | String.t()]}

  @doc """
  Parse an XML string into simple form.

  Returns `{:ok, root_element}` on success, `{:error, exception}` on failure.

  ## Options

    * `:cdata_as_characters` - Merge CDATA into text content (default: `true`)

  """
  @spec parse_string(String.t(), keyword()) :: {:ok, element()} | {:error, any()}
  def parse_string(xml, _opts \\ []) do
    case RustyXML.Native.parse_to_simple_form(xml) do
      {:ok, tree} -> {:ok, tree}
      {:error, _} = err -> err
    end
  end

  @doc """
  Parse a stream of XML chunks into simple form.

  Accumulates all chunks in Rust, then validates, indexes, and builds
  the SimpleForm tree in one pass. Minimal BEAM memory during accumulation.

  ## Examples

      File.stream!("large.xml", [], 64 * 1024)
      |> RustyXML.SimpleForm.parse_stream()
      #=> {:ok, {"root", [], [...]}}

  """
  @spec parse_stream(Enumerable.t(), keyword()) :: {:ok, element()} | {:error, any()}
  def parse_stream(stream, _opts \\ []) do
    acc = RustyXML.Native.accumulator_new()
    Enum.each(stream, &RustyXML.Native.accumulator_feed(acc, &1))
    RustyXML.Native.accumulator_to_simple_form(acc)
  end

  # Keep the handler module for backwards compatibility if anyone
  # references it, but it's no longer used by parse_string/2.
  defmodule Handler do
    @moduledoc false
    @behaviour RustyXML.Handler

    @impl true
    def handle_event(:start_document, _prolog, state), do: {:ok, state}

    def handle_event(:start_element, {name, attrs}, stack) do
      {:ok, [{name, attrs, []} | stack]}
    end

    def handle_event(:characters, _content, []) do
      # Whitespace at document root level — ignore
      {:ok, []}
    end

    def handle_event(:characters, content, [{name, attrs, children} | rest]) do
      {:ok, [{name, attrs, [content | children]} | rest]}
    end

    def handle_event(:cdata, content, [{name, attrs, children} | rest]) do
      {:ok, [{name, attrs, [content | children]} | rest]}
    end

    def handle_event(:end_element, _name, [current | []]) do
      {name, attrs, children} = current
      {:ok, [{name, attrs, Enum.reverse(children)}]}
    end

    def handle_event(:end_element, _name, [current, parent | rest]) do
      {name, attrs, children} = current
      {pname, pattrs, pchildren} = parent
      completed = {name, attrs, Enum.reverse(children)}
      {:ok, [{pname, pattrs, [completed | pchildren]} | rest]}
    end

    def handle_event(:end_document, _data, state), do: {:ok, state}
  end
end
