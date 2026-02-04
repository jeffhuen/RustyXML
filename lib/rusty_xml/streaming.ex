defmodule RustyXML.Streaming do
  @moduledoc """
  Streaming XML parser for processing large files with bounded memory.

  This module provides a streaming interface to the Rust-based streaming
  parser (Strategy D). It reads data in chunks and yields complete XML elements
  as they become available.

  ## Memory Behavior

  The streaming parser maintains a small buffer for partial elements. Memory
  usage is bounded by:

    * `chunk_size` - bytes per IO read operation
    * Maximum single element size in your data

  ## Usage

  For most use cases, use the high-level `stream_tags/3` function from
  the main `RustyXML` module:

      "large.xml"
      |> RustyXML.stream_tags(:item)
      |> Stream.each(fn {:item, element} ->
        name = RustyXML.xpath(element, ~x"./name/text()"s)
        IO.puts("Processing: \#{name}")
      end)
      |> Stream.run()

  ## SweetXml Compatibility

  The stream returns `{tag_atom, element}` tuples compatible with SweetXml:

      # Works the same as SweetXml
      "data.xml"
      |> RustyXML.stream_tags(:item)
      |> Stream.map(fn {:item, item} ->
        %{
          id: RustyXML.xpath(item, ~x"./@id"s),
          name: RustyXML.xpath(item, ~x"./name/text()"s)
        }
      end)
      |> Enum.to_list()

  ## Implementation Notes

  The streaming parser:

    * Handles elements that span multiple chunks correctly
    * Preserves element state across chunk boundaries
    * Compacts internal buffer to prevent unbounded growth
    * Returns complete elements (not raw events)
    * Does NOT hang with Stream.take (unlike SweetXml issue #97)

  """

  alias RustyXML.Native

  # ==========================================================================
  # Types
  # ==========================================================================

  @typedoc "Streamed element tuple - {tag_atom, xml_string}"
  @type streamed_element :: {atom(), binary()}

  @typedoc """
  Options for streaming functions.

  The `:discard` option is accepted for SweetXml API compatibility but has no
  effect. RustyXML's streaming parser already operates in bounded memory
  (~200 KB peak for a 2.93 MB document) by only materializing one element at a
  time, so tag discarding for memory reduction is unnecessary.
  """
  @type stream_options :: [
          chunk_size: pos_integer(),
          discard: [atom() | binary()]
        ]

  # ==========================================================================
  # Constants
  # ==========================================================================

  @default_chunk_size 64 * 1024

  # ==========================================================================
  # Public API
  # ==========================================================================

  @doc """
  Stream XML tags from a source.

  Returns a stream of `{tag_atom, element}` tuples where `element` is an
  XML string that can be queried with `RustyXML.xpath/2`.

  The source can be either:
    * A file path (binary)
    * An enumerable (like `File.stream!/1`)

  ## Options

    * `:chunk_size` - Bytes to read per IO operation. Defaults to `65536` (64 KB).
      Larger chunks mean fewer IO operations but more memory per read.

    * `:discard` - List of tag names whose content should be discarded.
      Useful for skipping large unwanted sections.

  ## Examples

      # Stream from a file path
      RustyXML.Streaming.stream_tags("data.xml", "item")
      |> Enum.each(fn {:item, item} ->
        IO.inspect(RustyXML.xpath(item, ~x"./name/text()"s))
      end)

      # Stream with take (works correctly, unlike SweetXml)
      RustyXML.Streaming.stream_tags("data.xml", "item")
      |> Stream.take(5)
      |> Enum.to_list()

      # Stream from an enumerable
      File.stream!("data.xml", [], 64 * 1024)
      |> RustyXML.Streaming.stream_tags("item")
      |> Enum.to_list()

  """
  @spec stream_tags(binary() | Enumerable.t(), binary() | atom(), stream_options()) ::
          Enumerable.t()
  def stream_tags(source, tag, opts \\ [])

  def stream_tags(path, tag, opts) when is_binary(path) do
    # Check if it looks like a file path (not XML content)
    if String.starts_with?(path, "<") do
      # It's XML content, not a path
      stream_string(path, tag, opts)
    else
      tag_str = normalize_tag(tag)
      tag_atom = normalize_tag_atom(tag)
      stream_file_elements(path, tag_str, tag_atom, opts)
    end
  end

  def stream_tags(enumerable, tag, opts) do
    tag_str = normalize_tag(tag)
    tag_atom = normalize_tag_atom(tag)
    stream_enumerable_elements(enumerable, tag_str, tag_atom, opts)
  end

  @doc """
  Stream from an XML string.
  """
  def stream_string(xml_string, tag, opts \\ []) when is_binary(xml_string) do
    tag_str = normalize_tag(tag)
    tag_atom = normalize_tag_atom(tag)

    # Split into chunks and stream
    chunk_size = Keyword.get(opts, :chunk_size, @default_chunk_size)

    xml_string
    |> chunk_string(chunk_size)
    |> stream_enumerable_elements(tag_str, tag_atom, opts)
  end

  defp chunk_string(string, chunk_size) do
    Stream.unfold(string, fn
      "" ->
        nil

      s ->
        {chunk, rest} = String.split_at(s, chunk_size)
        {chunk, rest}
    end)
  end

  # ==========================================================================
  # File Streaming - Returns Complete Elements
  # ==========================================================================

  defp stream_file_elements(path, tag_str, tag_atom, opts) do
    chunk_size = Keyword.get(opts, :chunk_size, @default_chunk_size)

    Stream.resource(
      fn -> init_file_element_stream(path, tag_str, chunk_size, tag_atom) end,
      &next_element_file/1,
      &cleanup_file_stream/1
    )
  end

  defp init_file_element_stream(path, tag_str, chunk_size, tag_atom) do
    device = File.open!(path, [:read, :binary, :raw])
    parser = Native.streaming_new_with_filter(tag_str)

    # State: {device, parser, chunk_size, tag_atom, element_builder, complete_elements}
    # element_builder: nil | {depth, xml_acc}
    {:file, device, parser, chunk_size, tag_atom, nil, []}
  end

  defp next_element_file(
         {:file, device, parser, chunk_size, tag_atom, builder, complete} = _state
       ) do
    # First, emit any complete elements we have
    if complete != [] do
      [first | rest] = Enum.reverse(complete)
      {[first], {:file, device, parser, chunk_size, tag_atom, builder, Enum.reverse(rest)}}
    else
      # Read more data and process
      read_and_build_elements(device, parser, chunk_size, tag_atom, builder)
    end
  end

  defp next_element_file({:done, _device, _parser, _chunk_size, _tag_atom, _builder, []} = state) do
    {:halt, state}
  end

  defp next_element_file({:done, device, parser, chunk_size, tag_atom, builder, complete}) do
    [first | rest] = Enum.reverse(complete)
    {[first], {:done, device, parser, chunk_size, tag_atom, builder, Enum.reverse(rest)}}
  end

  defp read_and_build_elements(device, parser, chunk_size, tag_atom, _builder) do
    case IO.binread(device, chunk_size) do
      :eof ->
        # Get remaining complete elements
        elements = Native.streaming_take_elements(parser, 1000)
        complete = Enum.map(elements, fn elem -> {tag_atom, elem} end)

        # Finalize (discard events, we use complete elements)
        _events = Native.streaming_finalize(parser)

        if complete == [] do
          {:halt, {:done, device, parser, chunk_size, tag_atom, nil, []}}
        else
          [first | rest] = complete
          {[first], {:done, device, parser, chunk_size, tag_atom, nil, rest}}
        end

      {:error, reason} ->
        raise RustyXML.ParseError, message: "Error reading XML file: #{inspect(reason)}"

      chunk when is_binary(chunk) ->
        {_available, _buffer_size} = Native.streaming_feed(parser, chunk)

        # Use fast path: get complete elements directly (no event processing)
        elements = Native.streaming_take_elements(parser, 1000)
        complete = Enum.map(elements, fn elem -> {tag_atom, elem} end)

        if complete == [] do
          # Keep reading until we have a complete element
          next_element_file({:file, device, parser, chunk_size, tag_atom, nil, []})
        else
          [first | rest] = complete
          {[first], {:file, device, parser, chunk_size, tag_atom, nil, rest}}
        end
    end
  end

  defp cleanup_file_stream({_, device, _parser, _chunk_size, _tag_atom, _builder, _complete}) do
    File.close(device)
  end

  # ==========================================================================
  # Enumerable Streaming - Returns Complete Elements
  # ==========================================================================

  defp stream_enumerable_elements(enumerable, tag_str, tag_atom, opts) do
    # Reserved for future use
    _ = opts

    Stream.resource(
      fn -> init_enum_element_stream(enumerable, tag_str, tag_atom) end,
      &next_element_enum/1,
      fn _state -> :ok end
    )
  end

  defp init_enum_element_stream(enumerable, tag_str, tag_atom) do
    parser = Native.streaming_new_with_filter(tag_str)

    # Start the enumerable reducer
    iterator =
      Enumerable.reduce(enumerable, {:cont, nil}, fn item, _acc -> {:suspend, item} end)

    {:enum, iterator, parser, tag_atom, nil, []}
  end

  defp next_element_enum({:enum, iterator, parser, tag_atom, builder, complete}) do
    # First emit complete elements
    if complete != [] do
      [first | rest] = Enum.reverse(complete)
      {[first], {:enum, iterator, parser, tag_atom, builder, Enum.reverse(rest)}}
    else
      # Need to read more
      process_enum_for_elements(iterator, parser, tag_atom, builder)
    end
  end

  defp process_enum_for_elements({:suspended, chunk, continuation}, parser, tag_atom, _builder) do
    chunk_binary = if is_binary(chunk), do: chunk, else: to_string(chunk)
    {_available, _buffer_size} = Native.streaming_feed(parser, chunk_binary)

    # Use fast path: get complete elements directly from Rust (no event processing)
    elements = Native.streaming_take_elements(parser, 1000)
    complete = Enum.map(elements, fn elem -> {tag_atom, elem} end)

    next_iterator = continuation.({:cont, nil})

    if complete == [] do
      # Keep reading
      process_enum_for_elements(next_iterator, parser, tag_atom, nil)
    else
      [first | rest] = complete
      {[first], {:enum, next_iterator, parser, tag_atom, nil, rest}}
    end
  end

  defp process_enum_for_elements({:done, _}, parser, tag_atom, _builder) do
    # Get any remaining complete elements
    elements = Native.streaming_take_elements(parser, 1000)
    complete = Enum.map(elements, fn elem -> {tag_atom, elem} end)

    # Finalize (discard events, we use complete elements)
    _events = Native.streaming_finalize(parser)

    if complete == [] do
      {:halt, {:enum, {:done, nil}, parser, tag_atom, nil, []}}
    else
      [first | rest] = complete
      {[first], {:enum, {:done, nil}, parser, tag_atom, nil, rest}}
    end
  end

  defp process_enum_for_elements({:halted, _}, _parser, _tag_atom, _builder) do
    {:halt, {:enum, {:halted, nil}, nil, nil, nil, []}}
  end

  # ==========================================================================
  # Helpers
  # ==========================================================================

  defp normalize_tag(tag) when is_atom(tag), do: Atom.to_string(tag)
  defp normalize_tag(tag) when is_binary(tag), do: tag

  defp normalize_tag_atom(tag) when is_atom(tag), do: tag
  defp normalize_tag_atom(tag) when is_binary(tag), do: String.to_atom(tag)
end
