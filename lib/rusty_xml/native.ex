defmodule RustyXML.Native do
  @moduledoc """
  Low-level NIF bindings for XML parsing.

  This module provides direct access to the Rust NIF functions. For normal use,
  prefer the higher-level `RustyXML` module with its `~x` sigil support.

  ## Strategies

  The module exposes multiple parsing strategies:

    * `parse_events/1` - Zero-copy event parser (Strategy A)
    * `parse/1` - DOM parser returning document reference (Strategy C)
    * `xpath_query/2` - Execute XPath on a document
    * `parse_and_xpath/2` - Parse and query in one call
    * `streaming_*` functions - Stateful streaming parser (Strategy D)
    * `xpath_parallel/2` - Parallel XPath execution (Strategy E)

  ## Strategy Selection

  | Strategy | Use Case | Memory Model |
  |----------|----------|--------------|
  | `parse_events/1` | Event-based processing | Copy per event |
  | `parse/1` + `xpath_query/2` | Multiple queries | Cached DOM |
  | `parse_and_xpath/2` | Single query | Temporary DOM |
  | `streaming_*` | Large files | Bounded memory |
  | `xpath_parallel/2` | Multiple queries | Parallel execution |

  ## Scheduler Behaviour

  NIFs that parse raw XML input run on the dirty CPU scheduler to avoid
  blocking BEAM schedulers. These are: `parse_events/1`, `parse/1`,
  `parse_strict/1`, `parse_and_xpath/2`, `xpath_with_subspecs/3`, and
  `xpath_string_value/2`. Query NIFs on pre-parsed documents
  (`xpath_query/2`, `xpath_lazy/2`, etc.) run on normal schedulers
  because they perform sub-millisecond lookups on cached data.

  ## Error Handling

  Functions that access `Mutex`-protected resources (documents, streaming
  parsers) return `{:error, :mutex_poisoned}` if the mutex has been
  poisoned by a prior panic. Under normal operation this never occurs.

  """

  version = Mix.Project.config()[:version]

  use RustlerPrecompiled,
    otp_app: :rusty_xml,
    crate: "rustyxml",
    base_url: "https://github.com/jeffhuen/rustyxml/releases/download/v#{version}",
    force_build: System.get_env("FORCE_RUSTYXML_BUILD") in ["1", "true"],
    nif_versions: ["2.15", "2.16", "2.17"],
    targets:
      Enum.uniq(
        ["aarch64-apple-darwin", "x86_64-apple-darwin"] ++
          RustlerPrecompiled.Config.default_targets()
      ),
    version: version

  # ==========================================================================
  # Types
  # ==========================================================================

  @typedoc "Opaque reference to a parsed XML document"
  @opaque document_ref :: reference()

  @typedoc "Opaque reference to a streaming parser"
  @opaque parser_ref :: reference()

  @typedoc "XML event from parser"
  @type xml_event ::
          {:start_element, binary(), [{binary(), binary()}]}
          | {:end_element, binary()}
          | {:empty_element, binary(), [{binary(), binary()}]}
          | {:text, binary()}
          | {:cdata, binary()}
          | {:comment, binary()}

  # ==========================================================================
  # Strategy A: Event Parser
  # ==========================================================================

  @doc """
  Parse XML and return a list of events.

  Runs on the dirty CPU scheduler since parse time scales with input size.

  Events are returned as tuples:
    * `{:start_element, name, attributes}` - Element start tag
    * `{:end_element, name}` - Element end tag
    * `{:empty_element, name, attributes}` - Empty element (self-closing)
    * `{:text, content}` - Text content
    * `{:cdata, content}` - CDATA section
    * `{:comment, content}` - Comment

  ## Examples

      iex> RustyXML.Native.parse_events("<root><child/></root>")
      [
        {:start_element, "root", []},
        {:empty_element, "child", []},
        {:end_element, "root"}
      ]

  """
  @spec parse_events(binary()) :: [xml_event()]
  def parse_events(_xml), do: :erlang.nif_error(:nif_not_loaded)

  # ==========================================================================
  # Strategy C: DOM Parser with XPath
  # ==========================================================================

  @doc """
  Parse XML into a DOM document (lenient mode).

  Runs on the dirty CPU scheduler since parse time scales with input size.

  Returns an opaque document reference that can be used with `xpath_query/2`
  and `get_root/1`. The document is cached and can be queried multiple times.

  This is lenient mode - it accepts malformed XML where possible.

  ## Examples

      doc = RustyXML.Native.parse("<root><item id=\"1\"/></root>")
      RustyXML.Native.xpath_query(doc, "//item")

  """
  @spec parse(binary()) :: document_ref()
  def parse(_xml), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Parse XML into a DOM document (strict mode).

  Runs on the dirty CPU scheduler since parse time scales with input size.

  Returns `{:ok, document_ref}` on success, or `{:error, reason}` if the
  document is not well-formed per XML 1.0 specification.

  Strict mode validates:
  - Element and attribute names (proper name characters)
  - Comment content (no `--` sequences)
  - Text content (no unescaped `]]>`)
  - Character references (valid Unicode codepoints)

  ## Examples

      {:ok, doc} = RustyXML.Native.parse_strict("<root>valid</root>")

      {:error, reason} = RustyXML.Native.parse_strict("<1invalid/>")
      #=> {:error, "Invalid name: must start with letter or underscore"}

  """
  @spec parse_strict(binary()) :: {:ok, document_ref()} | {:error, binary()}
  def parse_strict(_xml), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Execute an XPath query on a parsed document.

  Returns the result based on the XPath expression:
    * Node-set queries return a list of element tuples
    * String queries return a string
    * Number queries return a float
    * Boolean queries return true/false
    * `{:error, :mutex_poisoned}` if the document mutex is poisoned

  Returns `nil` if the document reference has no document.

  ## Examples

      doc = RustyXML.Native.parse("<root><item>text</item></root>")
      RustyXML.Native.xpath_query(doc, "//item")
      #=> [{:element, "item", [], ["text"]}]

  """
  @spec xpath_query(document_ref(), binary()) :: term() | nil | {:error, :mutex_poisoned}
  def xpath_query(_doc, _xpath), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Execute an XPath query returning XML strings for node sets (fast path).

  Instead of building nested Elixir tuples for each element, this returns
  the serialized XML string for each node. Much faster for queries returning
  many elements.

  ## Examples

      doc = RustyXML.Native.parse("<root><item>text</item></root>")
      RustyXML.Native.xpath_query_raw(doc, "//item")
      #=> ["<item>text</item>"]

  """
  @spec xpath_query_raw(document_ref(), binary()) ::
          [binary()] | term() | {:error, :mutex_poisoned}
  def xpath_query_raw(_doc, _xpath), do: :erlang.nif_error(:nif_not_loaded)

  # ==========================================================================
  # Lazy XPath (Zero-copy result sets)
  # ==========================================================================

  @typedoc "Opaque reference to an XPath result set stored in Rust memory"
  @type result_ref :: reference()

  @doc """
  Execute XPath query returning a lazy result set.

  Unlike `xpath_query/2`, this does NOT build BEAM terms for the results.
  The matched nodes stay in Rust memory and can be accessed on-demand
  using `result_count/1`, `result_text/2`, `result_attr/3`, etc.

  This is much faster for queries returning many nodes when you only
  need to access a subset of the data.

  ## Examples

      doc = RustyXML.Native.parse(large_xml)
      result = RustyXML.Native.xpath_lazy(doc, "//item")
      count = RustyXML.Native.result_count(result)  # => 10000
      first_name = RustyXML.Native.result_text(result, 0)  # Only builds 1 term

  """
  @spec xpath_lazy(document_ref(), binary()) ::
          result_ref() | {:error, binary()} | {:error, :mutex_poisoned}
  def xpath_lazy(_doc, _xpath), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Get the number of nodes in a lazy result set.
  """
  @spec result_count(result_ref()) :: non_neg_integer()
  def result_count(_result), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Get the text content of a node at the given index.

  For text nodes, returns the text directly.
  For elements, returns concatenated text of all descendant text nodes.
  Returns `{:error, :mutex_poisoned}` if the document mutex is poisoned.
  """
  @spec result_text(result_ref(), non_neg_integer()) ::
          binary() | nil | {:error, :mutex_poisoned}
  def result_text(_result, _index), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Get an attribute value from a node at the given index.

  Returns `{:error, :mutex_poisoned}` if the document mutex is poisoned.
  """
  @spec result_attr(result_ref(), non_neg_integer(), binary()) ::
          binary() | nil | {:error, :mutex_poisoned}
  def result_attr(_result, _index, _attr_name), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Get the element name of a node at the given index.

  Returns `{:error, :mutex_poisoned}` if the document mutex is poisoned.
  """
  @spec result_name(result_ref(), non_neg_integer()) ::
          binary() | nil | {:error, :mutex_poisoned}
  def result_name(_result, _index), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Get the full node at the given index as a BEAM term.

  This builds the full nested structure, so use sparingly.
  Prefer `result_text/2` and `result_attr/3` for better performance.

  Returns `{:error, :mutex_poisoned}` if the document mutex is poisoned.
  """
  @spec result_node(result_ref(), non_neg_integer()) ::
          term() | nil | {:error, :mutex_poisoned}
  def result_node(_result, _index), do: :erlang.nif_error(:nif_not_loaded)

  # ==========================================================================
  # Batch Accessors (single NIF call for multiple items)
  # ==========================================================================

  @doc """
  Get text content for a range of nodes (single NIF call).

  More efficient than calling `result_text/2` in a loop. The range is clamped
  to the actual result count — indices beyond the result set are not iterated,
  so the returned list may be shorter than `count`. Overflow of `start + count`
  is handled safely via saturation.

  Returns `{:error, :mutex_poisoned}` if the document mutex is poisoned.

  ## Examples

      result = RustyXML.Native.xpath_lazy(doc, "//item")
      texts = RustyXML.Native.result_texts(result, 0, 10)  # First 10 texts

  """
  @spec result_texts(result_ref(), non_neg_integer(), non_neg_integer()) ::
          [binary() | nil] | {:error, :mutex_poisoned}
  def result_texts(_result, _start, _count), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Get attribute values for a range of nodes (single NIF call).

  The range is clamped to the actual result count (see `result_texts/3`).
  Returns `{:error, :mutex_poisoned}` if the document mutex is poisoned.

  ## Examples

      result = RustyXML.Native.xpath_lazy(doc, "//item")
      ids = RustyXML.Native.result_attrs(result, "id", 0, 10)  # First 10 @id values

  """
  @spec result_attrs(result_ref(), binary(), non_neg_integer(), non_neg_integer()) ::
          [binary() | nil] | {:error, :mutex_poisoned}
  def result_attrs(_result, _attr_name, _start, _count), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Extract multiple fields from each node in a range (single NIF call).

  Returns a list of maps with the requested fields. Much more efficient
  than calling individual accessors. The range is clamped to the actual
  result count (see `result_texts/3`).

  Returns `{:error, :mutex_poisoned}` if the document mutex is poisoned.

  ## Examples

      result = RustyXML.Native.xpath_lazy(doc, "//item")
      data = RustyXML.Native.result_extract(result, 0, 10, ["id", "category"], true)
      #=> [%{name: "item", text: "...", id: "1", category: "cat1"}, ...]

  ## Parameters

      - result: The lazy result reference
      - start: Starting index
      - count: Number of items to extract
      - attr_names: List of attribute names to include
      - include_text: Whether to include text content

  """
  @spec result_extract(result_ref(), non_neg_integer(), non_neg_integer(), [binary()], boolean()) ::
          [map()] | {:error, :mutex_poisoned}
  def result_extract(_result, _start, _count, _attr_names, _include_text),
    do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Parse XML and execute an XPath query in one call.

  Runs on the dirty CPU scheduler since it parses raw XML input.

  More efficient than `parse/1` + `xpath_query/2` for single queries
  since it doesn't create a persistent document reference.

  ## Examples

      RustyXML.Native.parse_and_xpath("<root><item/></root>", "//item")

  """
  @spec parse_and_xpath(binary(), binary()) :: term()
  def parse_and_xpath(_xml, _xpath), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Get the root element of a parsed document.

  Returns the root element as a tuple:
  `{:element, name, attributes, children}`

  ## Examples

      doc = RustyXML.Native.parse("<root attr=\"value\"><child/></root>")
      RustyXML.Native.get_root(doc)
      #=> {:element, "root", [{"attr", "value"}], [...]}

  """
  @spec get_root(document_ref()) :: term() | nil | {:error, :mutex_poisoned}
  def get_root(_doc), do: :erlang.nif_error(:nif_not_loaded)

  # ==========================================================================
  # XPath with Subspecs (for xpath/3 nesting)
  # ==========================================================================

  @doc """
  Execute parent XPath and evaluate subspecs for each result node.

  Runs on the dirty CPU scheduler since it parses raw XML input.

  Returns a list of maps with each subspec evaluated relative to the parent nodes.

  ## Examples

      xml = "<items><item><id>1</id><name>A</name></item></items>"
      RustyXML.Native.xpath_with_subspecs(xml, "//item", [{"id", "./id/text()"}, {"name", "./name/text()"}])
      #=> [%{id: "1", name: "A"}]

  """
  @spec xpath_with_subspecs(binary(), binary(), [{binary(), binary()}]) :: [map()]
  def xpath_with_subspecs(_xml, _parent_xpath, _subspecs), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Execute XPath and return string value of result.

  Runs on the dirty CPU scheduler since it parses raw XML input.
  For node-sets, returns text content of first node.

  ## Examples

      RustyXML.Native.xpath_string_value("<root>hello</root>", "//root/text()")
      #=> "hello"

  """
  @spec xpath_string_value(binary(), binary()) :: binary()
  def xpath_string_value(_xml, _xpath), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Execute XPath on document reference and return string value.

  Returns `{:error, :mutex_poisoned}` if the document mutex is poisoned.
  """
  @spec xpath_string_value_doc(document_ref(), binary()) ::
          binary() | {:error, :mutex_poisoned}
  def xpath_string_value_doc(_doc, _xpath), do: :erlang.nif_error(:nif_not_loaded)

  # ==========================================================================
  # Strategy D: Streaming Parser
  # ==========================================================================

  @doc """
  Create a new streaming XML parser.

  The streaming parser processes XML in chunks with bounded memory usage.

  ## Examples

      parser = RustyXML.Native.streaming_new()
      RustyXML.Native.streaming_feed(parser, "<root>")
      RustyXML.Native.streaming_feed(parser, "<item/></root>")
      events = RustyXML.Native.streaming_take_events(parser, 100)

  """
  @spec streaming_new() :: parser_ref()
  def streaming_new, do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Create a streaming parser with a tag filter.

  Only events for the specified tag name will be emitted.
  Useful for extracting specific elements from large documents.

  ## Examples

      parser = RustyXML.Native.streaming_new_with_filter("item")
      RustyXML.Native.streaming_feed(parser, "<root><item/><other/></root>")
      # Only item events will be returned

  """
  @spec streaming_new_with_filter(binary()) :: parser_ref()
  def streaming_new_with_filter(_tag), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Feed a chunk of XML data to the streaming parser.

  Returns `{available_events, buffer_size}` on success, or
  `{:error, :mutex_poisoned}` if the parser mutex is poisoned.
  """
  @spec streaming_feed(parser_ref(), binary()) ::
          {non_neg_integer(), non_neg_integer()} | {:error, :mutex_poisoned}
  def streaming_feed(_parser, _chunk), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Take up to `max` events from the streaming parser.

  Returns `{:error, :mutex_poisoned}` if the parser mutex is poisoned.
  """
  @spec streaming_take_events(parser_ref(), non_neg_integer()) ::
          [xml_event()] | {:error, :mutex_poisoned}
  def streaming_take_events(_parser, _max), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Finalize the streaming parser and get remaining events.

  Returns `{:error, :mutex_poisoned}` if the parser mutex is poisoned.
  """
  @spec streaming_finalize(parser_ref()) :: [xml_event()] | {:error, :mutex_poisoned}
  def streaming_finalize(_parser), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Get streaming parser status.

  Returns `{available_events, buffer_size, has_pending}` on success, or
  `{:error, :mutex_poisoned}` if the parser mutex is poisoned.
  """
  @spec streaming_status(parser_ref()) ::
          {non_neg_integer(), non_neg_integer(), boolean()} | {:error, :mutex_poisoned}
  def streaming_status(_parser), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Take up to `max` complete elements from the streaming parser.

  Returns a list of XML binaries for complete elements. This is faster than
  using events because the element strings are built in Rust without needing
  reconstruction in Elixir.

  """
  @spec streaming_take_elements(parser_ref(), non_neg_integer()) ::
          [binary()] | {:error, :mutex_poisoned}
  def streaming_take_elements(_parser, _max), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Get number of available complete elements.

  """
  @spec streaming_available_elements(parser_ref()) ::
          non_neg_integer() | {:error, :mutex_poisoned}
  def streaming_available_elements(_parser), do: :erlang.nif_error(:nif_not_loaded)

  # ==========================================================================
  # Strategy E: Parallel XPath
  # ==========================================================================

  @doc """
  Execute multiple XPath queries in parallel.

  Uses Rayon thread pool for parallel evaluation. Runs on dirty CPU
  scheduler to avoid blocking BEAM schedulers.

  ## Examples

      doc = RustyXML.Native.parse("<root><a/><b/><c/></root>")
      RustyXML.Native.xpath_parallel(doc, ["//a", "//b", "//c"])

  """
  @spec xpath_parallel(document_ref(), [binary()]) :: [term()] | {:error, :mutex_poisoned}
  def xpath_parallel(_doc, _xpaths), do: :erlang.nif_error(:nif_not_loaded)

  # ==========================================================================
  # Memory Tracking
  # ==========================================================================

  @doc """
  Get current Rust heap allocation in bytes.

  Requires `memory_tracking` Cargo feature. Returns `0` otherwise.

  """
  @spec get_rust_memory() :: non_neg_integer()
  def get_rust_memory, do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Get peak Rust heap allocation since last reset.

  """
  @spec get_rust_memory_peak() :: non_neg_integer()
  def get_rust_memory_peak, do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Reset memory tracking statistics.

  Returns `{current_bytes, previous_peak_bytes}`.

  """
  @spec reset_rust_memory_stats() :: {non_neg_integer(), non_neg_integer()}
  def reset_rust_memory_stats, do: :erlang.nif_error(:nif_not_loaded)
end
