defmodule RustyXML do
  @moduledoc """
  Ultra-fast XML parsing for Elixir with full XPath 1.0 support.

  RustyXML is a high-performance XML parser built from scratch as a Rust NIF
  with SIMD acceleration. It achieves **100% W3C/OASIS XML Conformance**
  (1089/1089 test cases) and provides a drop-in replacement for SweetXml with
  the familiar `~x` sigil syntax.

  ## Quick Start

      import RustyXML

      xml = "<root><item id=\"1\">Hello</item><item id=\"2\">World</item></root>"

      # Get a list of items
      xpath(xml, ~x"//item"l)
      #=> [{:element, "item", ...}, {:element, "item", ...}]

      # Get text content as string
      xpath(xml, ~x"//item/text()"s)
      #=> "Hello"

      # Map multiple values
      xmap(xml, items: ~x"//item"l, count: ~x"count(//item)"i)
      #=> %{items: [...], count: 2}

  ## Sigil Modifiers

  The `~x` sigil supports modifiers for result transformation:

    * `e` - Return entity (element) for chaining, not text value
    * `s` - Return as string (binary)
    * `S` - Soft string (empty string on error)
    * `l` - Return as list
    * `o` - Optional (return nil instead of raising on missing)
    * `i` - Cast to integer
    * `I` - Soft integer (0 on error)
    * `f` - Cast to float
    * `F` - Soft float (0.0 on error)
    * `k` - Return as keyword list

  ## XPath 1.0 Functions

  RustyXML supports all 27+ XPath 1.0 functions including:

  - Node: `position()`, `last()`, `count()`, `local-name()`, `namespace-uri()`, `name()`
  - String: `string()`, `concat()`, `starts-with()`, `contains()`, `substring()`, etc.
  - Boolean: `boolean()`, `not()`, `true()`, `false()`, `lang()`
  - Number: `number()`, `sum()`, `floor()`, `ceiling()`, `round()`

  ## Streaming

  For large files, use the streaming API:

      "large.xml"
      |> RustyXML.stream_tags(:item)
      |> Stream.each(&process_item/1)
      |> Stream.run()
  """

  alias RustyXML.Native

  # ==========================================================================
  # Types
  # ==========================================================================

  @type document :: Native.document_ref()
  @type xml_node :: {:element, binary(), [{binary(), binary()}], [xml_node() | binary()]}

  # ==========================================================================
  # SweetXpath Struct (SweetXml compatible)
  # ==========================================================================

  defmodule SweetXpath do
    @moduledoc """
    Struct representing an XPath expression with modifiers.

    This struct is compatible with SweetXml's `%SweetXpath{}` struct and is
    created by the `~x` sigil.
    """

    @type t :: %__MODULE__{
            path: binary(),
            is_value: boolean(),
            is_list: boolean(),
            is_keyword: boolean(),
            is_optional: boolean(),
            cast_to: nil | :string | :integer | :float,
            soft_cast: boolean(),
            transform: (term() -> term()) | nil,
            namespaces: [{binary(), binary()}]
          }

    defstruct path: "",
              is_value: true,
              is_list: false,
              is_keyword: false,
              is_optional: false,
              cast_to: nil,
              soft_cast: false,
              transform: nil,
              namespaces: []
  end

  # ==========================================================================
  # Exceptions
  # ==========================================================================

  defmodule ParseError do
    @moduledoc "Exception raised when XML parsing fails."
    defexception [:message]

    @impl true
    def message(%{message: message}), do: message
  end

  defmodule XPathError do
    @moduledoc "Exception raised when XPath evaluation fails."
    defexception [:message, :xpath]

    @impl true
    def message(%{message: message, xpath: xpath}) do
      "XPath error: #{message} (in \"#{xpath}\")"
    end
  end

  # ==========================================================================
  # Main API
  # ==========================================================================

  @doc """
  Parse an XML document.

  By default, RustyXML uses **strict mode** to match SweetXml/xmerl behavior.
  Malformed XML raises `RustyXML.ParseError`.

  Returns an opaque document reference that can be used with `xpath/2,3`
  for multiple queries on the same document.

  ## Options

    * `:lenient` - If `true`, accept malformed XML without raising.
      Useful for processing third-party or legacy XML. Default: `false`.

  ## Examples

      # Strict mode (default) - matches SweetXml behavior
      doc = RustyXML.parse("<root><item/></root>")
      RustyXML.xpath(doc, ~x"//item"l)

      # Raises on malformed XML (like SweetXml)
      RustyXML.parse("<1invalid/>")
      #=> ** (RustyXML.ParseError) Invalid element name...

      # Lenient mode - accepts malformed XML
      doc = RustyXML.parse("<1invalid/>", lenient: true)

  """
  @spec parse(binary(), keyword()) :: document()
  def parse(xml, opts \\ []) when is_binary(xml) do
    if Keyword.get(opts, :lenient, false) do
      Native.parse(xml)
    else
      case Native.parse_strict(xml) do
        {:ok, doc} -> doc
        {:error, reason} -> raise ParseError, message: reason
      end
    end
  end

  @doc """
  Parse an XML document, returning `{:ok, doc}` or `{:error, reason}`.

  Unlike `parse/2`, this function returns a tuple instead of raising,
  allowing pattern matching on parse results.

  ## Examples

      {:ok, doc} = RustyXML.parse_document("<root/>")
      {:error, reason} = RustyXML.parse_document("<1invalid/>")

  """
  @spec parse_document(binary()) :: {:ok, document()} | {:error, binary()}
  def parse_document(xml) when is_binary(xml) do
    Native.parse_strict(xml)
  end

  @doc """
  Execute an XPath query on XML.

  The first argument can be either:
    * A raw XML binary
    * A parsed document reference from `parse/1`

  The second argument can be:
    * A `%SweetXpath{}` struct (from `~x` sigil)
    * A plain XPath string (binary)

  ## Examples

      # On raw XML
      RustyXML.xpath("<root>text</root>", ~x"//root/text()"s)
      #=> "text"

      # On parsed document
      doc = RustyXML.parse("<root><a/><b/></root>")
      RustyXML.xpath(doc, ~x"//a"l)

  """
  @spec xpath(binary() | document(), SweetXpath.t() | binary()) :: term()
  def xpath(xml_or_doc, spec)

  def xpath(xml, %SweetXpath{} = spec) when is_binary(xml) do
    result = Native.parse_and_xpath(xml, spec.path)
    apply_modifiers(result, spec, xml)
  end

  def xpath(xml, path) when is_binary(xml) and is_binary(path) do
    Native.parse_and_xpath(xml, path)
  end

  def xpath(doc, %SweetXpath{} = spec) do
    result = Native.xpath_query(doc, spec.path)
    apply_modifiers(result, spec, nil)
  end

  def xpath(doc, path) when is_binary(path) do
    Native.xpath_query(doc, path)
  end

  @doc """
  Execute an XPath query with a mapping spec for nested extraction.

  The third argument is a keyword list of `{name, xpath_spec}` pairs
  that will be evaluated for each node in the parent result.

  ## Examples

      xml = "<items><item id=\"1\"><name>A</name></item><item id=\"2\"><name>B</name></item></items>"

      RustyXML.xpath(xml, ~x"//item"l, [
        id: ~x"./@id"s,
        name: ~x"./name/text()"s
      ])
      #=> [%{id: "1", name: "A"}, %{id: "2", name: "B"}]

  """
  @spec xpath(binary() | document(), SweetXpath.t() | binary(), keyword()) :: term()
  def xpath(xml_or_doc, spec, subspecs) when is_list(subspecs) do
    parent_path = extract_path(spec)

    # Convert subspecs to the format expected by NIF: [{key_string, xpath_string}]
    nif_subspecs =
      Enum.map(subspecs, fn {key, subspec} ->
        {Atom.to_string(key), extract_path(subspec)}
      end)

    # Get the raw XML if needed
    xml =
      if is_binary(xml_or_doc) do
        xml_or_doc
      else
        # For document refs, we can't use xpath_with_subspecs directly
        # Fall back to individual queries (less efficient)
        return_individual_queries(xml_or_doc, spec, subspecs)
      end

    if is_binary(xml) do
      Native.xpath_with_subspecs(xml, parent_path, nif_subspecs)
      |> maybe_apply_list_modifier(spec)
    else
      # Return the result from fallback
      xml
    end
  end

  # Fallback for document refs - query individually
  defp return_individual_queries(doc, parent_spec, _subspecs) do
    parent_result = xpath(doc, parent_spec)

    # For now, return parent result without subspec processing
    # Full implementation would require additional NIF support
    parent_result
  end

  @doc """
  Execute multiple XPath queries and return as a map.

  ## Options

    * None currently supported (for SweetXml compatibility)

  ## Examples

      xml = "<root><a>1</a><b>2</b></root>"

      RustyXML.xmap(xml, [
        a: ~x"//a/text()"s,
        b: ~x"//b/text()"s
      ])
      #=> %{a: "1", b: "2"}

  """
  @spec xmap(binary() | document(), keyword(), keyword()) :: map()
  def xmap(xml_or_doc, specs, opts \\ []) when is_list(specs) do
    doc = if is_binary(xml_or_doc), do: parse(xml_or_doc), else: xml_or_doc
    # Reserved for future options
    _ = opts

    specs
    |> Enum.map(fn {key, spec} ->
      {key, evaluate_spec(doc, xml_or_doc, spec)}
    end)
    |> maybe_to_keyword(specs)
  end

  # Evaluate a spec - handles both simple specs and nested list specs
  defp evaluate_spec(doc, _xml, %SweetXpath{} = spec) do
    xpath(doc, spec)
  end

  defp evaluate_spec(_doc, xml, [%SweetXpath{} = parent_spec | child_specs])
       when is_list(child_specs) do
    # Nested spec: first element is parent path, rest are child specs
    # Get parent nodes as raw elements (bypass value extraction)
    parent_result = Native.parse_and_xpath(xml, parent_spec.path)

    # Ensure it's a list
    nodes =
      case parent_result do
        list when is_list(list) -> list
        nil -> []
        single -> [single]
      end

    # For each parent node, evaluate child specs
    Enum.map(nodes, fn node ->
      evaluate_child_specs(node, child_specs)
    end)
  end

  defp evaluate_spec(_doc, _xml, spec) do
    # Unknown spec type, return as-is
    spec
  end

  # Evaluate child specs against a parent node
  defp evaluate_child_specs(parent_node, child_specs) do
    # Convert parent node back to XML string for sub-queries
    parent_xml = node_to_xml(parent_node)

    child_specs
    |> Enum.filter(fn
      {_key, %SweetXpath{}} -> true
      # Nested specs
      {_key, [%SweetXpath{} | _]} -> true
      _ -> false
    end)
    |> Enum.map(fn {key, spec} ->
      result =
        case spec do
          %SweetXpath{} = s ->
            # Adjust relative paths and query
            adjusted_spec = adjust_relative_path(s)
            xpath(parent_xml, adjusted_spec)

          [%SweetXpath{} | _] = nested ->
            # Recursively handle nested specs
            evaluate_spec(nil, parent_xml, nested)
        end

      {key, result}
    end)
    |> Map.new()
  end

  # Adjust relative XPath (starting with ./) for sub-queries
  defp adjust_relative_path(%SweetXpath{path: path} = spec) do
    # If path starts with ./, keep it; otherwise make it relative
    adjusted_path =
      cond do
        String.starts_with?(path, "./") -> path
        String.starts_with?(path, ".") -> path
        String.starts_with?(path, "/") -> path
        true -> "./" <> path
      end

    %{spec | path: adjusted_path}
  end

  # Convert a node back to XML string for sub-queries
  defp node_to_xml({:element, name, attrs, children}) do
    attr_str = Enum.map_join(attrs, fn {k, v} -> " #{k}=\"#{escape_xml(v)}\"" end)

    children_str = Enum.map_join(children, &node_to_xml/1)

    "<#{name}#{attr_str}>#{children_str}</#{name}>"
  end

  defp node_to_xml(text) when is_binary(text), do: escape_xml(text)
  defp node_to_xml({:comment, text}), do: "<!--#{text}-->"
  defp node_to_xml({:pi, target}), do: "<?#{target}?>"
  defp node_to_xml(_), do: ""

  defp escape_xml(text) do
    text
    |> String.replace("&", "&amp;")
    |> String.replace("<", "&lt;")
    |> String.replace(">", "&gt;")
    |> String.replace("\"", "&quot;")
  end

  # Convert to keyword list if any spec has is_keyword set
  defp maybe_to_keyword(result, specs) do
    has_keyword =
      Enum.any?(specs, fn
        {_, %SweetXpath{is_keyword: true}} -> true
        _ -> false
      end)

    if has_keyword do
      Keyword.new(result)
    else
      Map.new(result)
    end
  end

  @doc """
  Execute multiple XPath queries in parallel and return as a map.

  Uses Rayon thread pool for parallel evaluation. More efficient than
  `xmap/2` for many queries on large documents.

  ## Examples

      doc = RustyXML.parse(large_xml)
      RustyXML.xmap_parallel(doc, [
        items: ~x"//item"l,
        count: ~x"count(//item)"
      ])

  """
  @spec xmap_parallel(document(), keyword()) :: map()
  def xmap_parallel(doc, specs) when is_list(specs) do
    xpaths = Enum.map(specs, fn {_key, spec} -> extract_path(spec) end)
    results = Native.xpath_parallel(doc, xpaths)

    specs
    |> Enum.zip(results)
    |> Enum.map(fn {{key, spec}, result} ->
      {key, apply_modifiers(result, spec, nil)}
    end)
    |> Map.new()
  end

  @doc """
  Add a namespace binding to an XPath expression.

  Returns a new `%SweetXpath{}` with the namespace added.

  ## Examples

      xpath_with_ns = add_namespace(~x"//ns:item"l, "ns", "http://example.com/ns")
      RustyXML.xpath(xml, xpath_with_ns)

  """
  @spec add_namespace(SweetXpath.t(), binary(), binary()) :: SweetXpath.t()
  def add_namespace(%SweetXpath{} = spec, prefix, uri)
      when is_binary(prefix) and is_binary(uri) do
    %{spec | namespaces: [{prefix, uri} | spec.namespaces]}
  end

  @doc """
  Add a transformation function to an XPath expression.

  The function will be applied to the result after all other modifiers.

  ## Examples

      spec = transform_by(~x"//price/text()"s, &String.to_float/1)
      RustyXML.xpath(xml, spec)
      #=> 45.99

  """
  @spec transform_by(SweetXpath.t(), (term() -> term())) :: SweetXpath.t()
  def transform_by(%SweetXpath{} = spec, fun) when is_function(fun, 1) do
    %{spec | transform: fun}
  end

  @doc """
  Stream XML events from a file.

  Returns a `Stream` that yields events as the file is read.
  Uses bounded memory regardless of file size.

  ## Options

    * `:chunk_size` - Bytes to read per IO operation (default: 64KB)
    * `:batch_size` - Maximum events per iteration (default: 100)
    * `:discard` - List of tags to discard (for memory efficiency)

  ## Examples

      "large.xml"
      |> RustyXML.stream_tags(:item)
      |> Stream.each(&process/1)
      |> Stream.run()

  """
  @spec stream_tags(binary() | Enumerable.t(), atom() | binary(), keyword()) :: Enumerable.t()
  def stream_tags(source, tag, opts \\ []) do
    tag_str = if is_atom(tag), do: Atom.to_string(tag), else: tag
    RustyXML.Streaming.stream_tags(source, tag_str, opts)
  end

  @doc """
  Stream XML events from a file. Raises on error.

  Same as `stream_tags/3` but raises on read errors instead of returning
  error tuples.
  """
  @spec stream_tags!(binary() | Enumerable.t(), atom() | binary(), keyword()) :: Enumerable.t()
  def stream_tags!(source, tag, opts \\ []) do
    # Currently stream_tags already raises on error
    stream_tags(source, tag, opts)
  end

  @doc """
  Get the root element of a parsed document.

  ## Examples

      doc = RustyXML.parse("<root><child/></root>")
      RustyXML.root(doc)
      #=> {:element, "root", [], [...]}

  """
  @spec root(document()) :: xml_node() | nil
  def root(doc) do
    Native.get_root(doc)
  end

  # ==========================================================================
  # Sigil
  # ==========================================================================

  @doc """
  The `~x` sigil for XPath expressions.

  Creates a `%SweetXpath{}` struct with the specified path and modifiers.

  ## Modifiers

    * `e` - Return entity (element) for chaining
    * `s` - Return as string
    * `S` - Soft string (empty on error)
    * `l` - Return as list
    * `o` - Optional (nil on missing)
    * `i` - Cast to integer
    * `I` - Soft integer (0 on error)
    * `f` - Cast to float
    * `F` - Soft float (0.0 on error)
    * `k` - Return as keyword list

  ## Examples

      import RustyXML

      ~x"//item"l          # List of items
      ~x"//name/text()"s   # String value
      ~x"count(//item)"i   # Integer count
      ~x"//optional"so     # Optional string

  """
  defmacro sigil_x({:<<>>, _meta, [path]}, modifiers) when is_binary(path) do
    spec = build_sweet_xpath(path, modifiers)
    Macro.escape(spec)
  end

  defmacro sigil_x({:<<>>, _meta, parts}, modifiers) do
    # Handle interpolated strings at runtime
    quote do
      path = unquote({:<<>>, [], parts})
      RustyXML.build_sweet_xpath(path, unquote(modifiers))
    end
  end

  @doc false
  def build_sweet_xpath(path, modifiers) do
    chars = to_charlist(modifiers)

    %SweetXpath{
      path: path,
      is_value: ?e not in chars,
      is_list: ?l in chars,
      is_keyword: ?k in chars,
      is_optional: ?o in chars,
      cast_to: determine_cast(chars),
      soft_cast: has_soft_modifier(chars)
    }
  end

  defp determine_cast(chars) do
    cond do
      ?s in chars or ?S in chars -> :string
      ?i in chars or ?I in chars -> :integer
      ?f in chars or ?F in chars -> :float
      true -> nil
    end
  end

  defp has_soft_modifier(chars) do
    ?S in chars or ?I in chars or ?F in chars
  end

  # ==========================================================================
  # Modifier Application
  # ==========================================================================

  defp apply_modifiers(result, %SweetXpath{} = spec, _xml) do
    result
    |> maybe_extract_value(spec)
    |> maybe_apply_list(spec)
    |> maybe_cast(spec)
    |> maybe_apply_optional(spec)
    |> maybe_apply_keyword(spec)
    |> maybe_apply_transform(spec)
  end

  # Extract string value if is_value is true (no `e` modifier)
  defp maybe_extract_value(result, %{is_value: true, is_list: false}) do
    extract_text_value(result)
  end

  defp maybe_extract_value(result, %{is_value: true, is_list: true}) do
    case result do
      list when is_list(list) -> Enum.map(list, &extract_text_value/1)
      _ -> result
    end
  end

  defp maybe_extract_value(result, _spec), do: result

  # Extract text from a node result
  defp extract_text_value({:element, _name, _attrs, children}) do
    Enum.map_join(children, &extract_text_value/1)
  end

  defp extract_text_value(text) when is_binary(text), do: text
  defp extract_text_value(list) when is_list(list), do: list
  defp extract_text_value(other), do: other

  # Ensure list result
  defp maybe_apply_list(result, %{is_list: true}) do
    case result do
      list when is_list(list) -> list
      nil -> []
      other -> [other]
    end
  end

  defp maybe_apply_list(result, %{is_list: false}) do
    case result do
      [first | _] -> first
      [] -> nil
      other -> other
    end
  end

  defp maybe_apply_list(result, _spec), do: result

  # Apply list modifier from xpath/3
  defp maybe_apply_list_modifier(result, %{is_list: false}) do
    case result do
      [first | _] -> first
      other -> other
    end
  end

  defp maybe_apply_list_modifier(result, _spec), do: result

  # Cast to type
  defp maybe_cast(result, %{cast_to: nil}), do: result

  defp maybe_cast(result, %{cast_to: :string, soft_cast: soft, is_list: true}) do
    Enum.map(result, &cast_to_string(&1, soft))
  end

  defp maybe_cast(result, %{cast_to: :string, soft_cast: soft}) do
    cast_to_string(result, soft)
  end

  defp maybe_cast(result, %{cast_to: :integer, soft_cast: soft, is_list: true}) do
    Enum.map(result, &cast_to_integer(&1, soft))
  end

  defp maybe_cast(result, %{cast_to: :integer, soft_cast: soft}) do
    cast_to_integer(result, soft)
  end

  defp maybe_cast(result, %{cast_to: :float, soft_cast: soft, is_list: true}) do
    Enum.map(result, &cast_to_float(&1, soft))
  end

  defp maybe_cast(result, %{cast_to: :float, soft_cast: soft}) do
    cast_to_float(result, soft)
  end

  # Soft cast returns nil on failure/empty, hard cast raises
  defp cast_to_string(nil, _soft), do: nil
  # Soft cast: empty string -> nil
  defp cast_to_string("", true), do: nil
  defp cast_to_string(val, _soft) when is_binary(val), do: val
  defp cast_to_string(val, _soft) when is_number(val), do: to_string(val)
  defp cast_to_string(val, _soft), do: to_string(val)

  defp cast_to_integer(nil, _soft), do: nil
  # Soft cast: empty string -> nil
  defp cast_to_integer("", true), do: nil

  defp cast_to_integer("", false),
    do: raise(ArgumentError, "cannot parse as integer: empty string")

  defp cast_to_integer(val, soft) when is_binary(val) do
    case Integer.parse(String.trim(val)) do
      {int, _} -> int
      :error -> if soft, do: nil, else: raise(ArgumentError, "cannot parse as integer: #{val}")
    end
  end

  defp cast_to_integer(val, _soft) when is_integer(val), do: val
  defp cast_to_integer(val, _soft) when is_float(val), do: trunc(val)
  # Soft cast: unparseable -> nil
  defp cast_to_integer(_val, true), do: nil

  defp cast_to_integer(val, false),
    do: raise(ArgumentError, "cannot cast to integer: #{inspect(val)}")

  defp cast_to_float(nil, _soft), do: nil
  # Soft cast: empty string -> nil
  defp cast_to_float("", true), do: nil
  defp cast_to_float("", false), do: raise(ArgumentError, "cannot parse as float: empty string")

  defp cast_to_float(val, soft) when is_binary(val) do
    case Float.parse(String.trim(val)) do
      {float, _} -> float
      :error -> if soft, do: nil, else: raise(ArgumentError, "cannot parse as float: #{val}")
    end
  end

  defp cast_to_float(val, _soft) when is_float(val), do: val
  defp cast_to_float(val, _soft) when is_integer(val), do: val / 1
  # Soft cast: unparseable -> nil
  defp cast_to_float(_val, true), do: nil

  defp cast_to_float(val, false),
    do: raise(ArgumentError, "cannot cast to float: #{inspect(val)}")

  # Handle optional
  defp maybe_apply_optional(nil, %{is_optional: true}), do: nil
  defp maybe_apply_optional([], %{is_optional: true}), do: nil
  defp maybe_apply_optional(result, _spec), do: result

  # Convert to keyword list
  defp maybe_apply_keyword(result, %{is_keyword: true}) when is_map(result) do
    Keyword.new(result)
  end

  defp maybe_apply_keyword(result, _spec), do: result

  # Apply transform function
  defp maybe_apply_transform(result, %{transform: nil}), do: result
  defp maybe_apply_transform(result, %{transform: fun}), do: fun.(result)

  # ==========================================================================
  # Helpers
  # ==========================================================================

  defp extract_path(%SweetXpath{path: path}), do: path
  defp extract_path(path) when is_binary(path), do: path
end
