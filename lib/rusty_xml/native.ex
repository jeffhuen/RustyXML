defmodule RustyXML.Native do
  @moduledoc """
  Low-level NIF bindings for XML parsing.

  This module provides direct access to the Rust NIF functions. For normal use,
  prefer the higher-level `RustyXML` module with its `~x` sigil support.

  ## Strategies

  The module exposes parsing strategies:

    * `parse/1` + `xpath_query/2` - Structural index with XPath (main path)
    * `streaming_*` - Stateful streaming parser for large files
    * `sax_parse/1` - SAX event parser

  ## Memory Efficiency

  The structural index (`parse/1`) uses ~4x input size vs SweetXml's ~600x.
  Strings are stored as byte offsets into the original input, not copies.

  ## Scheduler Behaviour

  NIFs that parse raw XML input run on the dirty CPU scheduler to avoid
  blocking BEAM schedulers. Query NIFs on pre-parsed documents run on
  normal schedulers for sub-millisecond lookups.
  """

  version = Mix.Project.config()[:version]

  # Accept both RUSTYXML_BUILD and FORCE_RUSTYXML_BUILD for forcing local compilation
  @force_build (System.get_env("RUSTYXML_BUILD") || System.get_env("FORCE_RUSTYXML_BUILD")) in [
                 "1",
                 "true"
               ]

  use RustlerPrecompiled,
    otp_app: :rusty_xml,
    crate: "rustyxml",
    base_url: "https://github.com/jeffhuen/rustyxml/releases/download/v#{version}",
    force_build: @force_build,
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

  @typedoc "Opaque reference to a parsed XML document (structural index)"
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
  # Main Parse Path: Structural Index + XPath
  # ==========================================================================

  @doc """
  Parse XML into a structural index document.

  Runs on the dirty CPU scheduler since parse time scales with input size.

  Returns an opaque document reference that can be used with `xpath_query/2`
  and `get_root/1`. The document is cached and can be queried multiple times.

  This is the primary parse function - uses ~4x input size memory.

  ## Examples

      doc = RustyXML.Native.parse("<root><item id=\"1\"/></root>")
      RustyXML.Native.xpath_query(doc, "//item")

  """
  @spec parse(binary()) :: document_ref()
  def parse(_xml), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Parse XML in strict mode (returns {:ok, doc} or {:error, reason}).

  Runs on the dirty CPU scheduler since parse time scales with input size.

  Returns `{:ok, document_ref}` on success, or `{:error, reason}` if the
  document is not well-formed per XML 1.0 specification.

  ## Examples

      {:ok, doc} = RustyXML.Native.parse_strict("<root>valid</root>")

      {:error, reason} = RustyXML.Native.parse_strict("<1invalid/>")

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

  ## Examples

      doc = RustyXML.Native.parse("<root><item>text</item></root>")
      RustyXML.Native.xpath_query(doc, "//item")
      #=> [{:element, "item", [], ["text"]}]

  """
  @spec xpath_query(document_ref(), binary()) :: term()
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
  @spec xpath_query_raw(document_ref(), binary()) :: [binary()] | term()
  def xpath_query_raw(_doc, _xpath), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Execute XPath query returning text values for node sets (optimized fast path).

  Instead of building nested Elixir tuples for each element, returns the
  concatenated text content of each node as a string. Much faster for the
  common case where `is_value: true` (no `e` modifier).

  For non-NodeSet results (numbers, strings, booleans), returns as-is.
  """
  @spec xpath_text_list(document_ref(), binary()) :: [binary()] | term()
  def xpath_text_list(_doc, _xpath), do: :erlang.nif_error(:nif_not_loaded)

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
  Parse and immediately query, returning text values for node sets.

  Optimized path for `is_value: true` — avoids building element tuples.
  """
  @spec parse_and_xpath_text(binary(), binary()) :: [binary()] | term()
  def parse_and_xpath_text(_xml, _xpath), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Get the root element of a parsed document.

  Returns the root element as a tuple:
  `{:element, name, attributes, children}`

  ## Examples

      doc = RustyXML.Native.parse("<root attr=\"value\"><child/></root>")
      RustyXML.Native.get_root(doc)
      #=> {:element, "root", [{"attr", "value"}], [...]}

  """
  @spec get_root(document_ref()) :: term() | nil
  def get_root(_doc), do: :erlang.nif_error(:nif_not_loaded)

  # ==========================================================================
  # XPath Helpers
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
  """
  @spec xpath_string_value_doc(document_ref(), binary()) :: binary()
  def xpath_string_value_doc(_doc, _xpath), do: :erlang.nif_error(:nif_not_loaded)

  # ==========================================================================
  # Streaming Parser
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
  # SimpleForm Parsing
  # ==========================================================================

  @doc """
  Parse XML directly into SimpleForm `{name, attrs, children}` tree.

  Bypasses the SAX event pipeline — builds the tree in Rust from the
  structural index, decoding entities as needed.

  Returns `{:ok, tree}` or `{:error, reason}`.
  """
  @spec parse_to_simple_form(binary()) :: {:ok, tuple()} | {:error, binary()}
  def parse_to_simple_form(_xml), do: :erlang.nif_error(:nif_not_loaded)

  # ==========================================================================
  # Document Accumulator (Streaming SimpleForm)
  # ==========================================================================

  @doc """
  Create a new document accumulator for streaming SimpleForm parsing.

  Returns an opaque accumulator reference.
  """
  @spec accumulator_new() :: reference()
  def accumulator_new, do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Feed a chunk of data to the document accumulator.
  """
  @spec accumulator_feed(reference(), binary()) :: :ok
  def accumulator_feed(_acc, _chunk), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Validate, index, and convert accumulated data to SimpleForm.

  Returns `{:ok, tree}` or `{:error, reason}`.
  """
  @spec accumulator_to_simple_form(reference()) :: {:ok, tuple()} | {:error, binary()}
  def accumulator_to_simple_form(_acc), do: :erlang.nif_error(:nif_not_loaded)

  # ==========================================================================
  # SAX Parsing
  # ==========================================================================

  @doc """
  Parse XML and return SAX events.

  Events are returned as tuples similar to Saxy's format.
  """
  @spec sax_parse(binary()) :: [tuple()]
  def sax_parse(_xml), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Parse XML and return SAX events in Saxy-compatible format.

  Events are emitted directly in Saxy format:
  - `{:start_element, {name, attrs}}`
  - `{:end_element, name}`
  - `{:characters, content}`
  - `{:cdata, content}`

  Comments and PIs are skipped. Empty elements emit start+end.
  """
  @spec sax_parse_saxy(binary(), boolean()) :: [tuple()]
  def sax_parse_saxy(_xml, _cdata_as_chars), do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Take events from streaming parser in Saxy-compatible format.
  """
  @spec streaming_take_saxy_events(reference(), non_neg_integer(), boolean()) ::
          [tuple()] | {:error, :mutex_poisoned}
  def streaming_take_saxy_events(_parser, _max, _cdata_as_chars),
    do: :erlang.nif_error(:nif_not_loaded)

  # ==========================================================================
  # Streaming SAX Parsing (chunk-by-chunk, bounded memory)
  # ==========================================================================

  @doc """
  Create a new streaming SAX parser.

  Returns an opaque parser reference for use with `streaming_feed_sax/3`.
  """
  @spec streaming_sax_new() :: reference()
  def streaming_sax_new, do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Feed a chunk and return SAX events as a compact binary.

  Events are binary-encoded instead of creating BEAM tuples in the NIF.
  Elixir decodes one event at a time via pattern matching — only one event
  tuple is ever live on the heap (matching Saxy's inline-handler profile).

  Format: sequence of `<<type::8, ...>>` where type 1=start, 2=end, 3=chars, 4=cdata.
  """
  @spec streaming_feed_sax(reference(), binary(), boolean()) :: binary()
  def streaming_feed_sax(_parser, _chunk, _cdata_as_chars),
    do: :erlang.nif_error(:nif_not_loaded)

  @doc """
  Finalize the streaming SAX parser, processing any remaining bytes.

  Returns final events as a compact binary (same format as `streaming_feed_sax/3`).
  """
  @spec streaming_finalize_sax(reference(), boolean()) :: binary()
  def streaming_finalize_sax(_parser, _cdata_as_chars),
    do: :erlang.nif_error(:nif_not_loaded)

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
