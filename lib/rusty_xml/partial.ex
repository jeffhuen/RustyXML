defmodule RustyXML.Partial do
  @moduledoc """
  Incremental XML parsing for scenarios where the complete document
  is not available upfront (e.g., socket streams, chunked HTTP).

  Drop-in replacement for `Saxy.Partial`.

  ## Example

      {:ok, partial} = RustyXML.Partial.new(MyHandler, initial_state)

      {:cont, partial} = RustyXML.Partial.parse(partial, "<root>")
      {:cont, partial} = RustyXML.Partial.parse(partial, "<item/>")
      {:cont, partial} = RustyXML.Partial.parse(partial, "</root>")

      {:ok, final_state} = RustyXML.Partial.terminate(partial)

  """

  alias RustyXML.EventTransformer
  alias RustyXML.Native

  defstruct [:parser, :handler, :state, :opts, :started]

  @type t :: %__MODULE__{
          parser: Native.parser_ref(),
          handler: module(),
          state: any(),
          opts: keyword(),
          started: boolean()
        }

  @doc """
  Create a new partial parser.
  """
  @spec new(module(), any(), keyword()) :: {:ok, t()}
  def new(handler, initial_state, opts \\ []) do
    partial = %__MODULE__{
      parser: Native.streaming_new(),
      handler: handler,
      state: initial_state,
      opts: opts,
      started: false
    }

    {:ok, partial}
  end

  @doc """
  Feed a chunk of XML data to the parser.

  Returns:
    * `{:cont, partial}` — more data expected
    * `{:halt, state}` — handler returned `{:stop, state}`
    * `{:error, reason}` — parse error

  """
  @spec parse(t(), binary()) :: {:cont, t()} | {:halt, any()} | {:error, any()}
  def parse(%__MODULE__{} = partial, chunk) when is_binary(chunk) do
    cdata_as_chars = Keyword.get(partial.opts, :cdata_as_characters, false)

    try do
      partial = maybe_send_start_document(partial)

      case Native.streaming_feed(partial.parser, chunk) do
        {:error, reason} ->
          {:error, reason}

        {_available, _buffer_size} ->
          partial = drain_events(partial, cdata_as_chars)
          {:cont, partial}
      end
    catch
      {:sax_stop, value} -> {:halt, value}
      {:sax_halt, value} -> {:halt, value}
    end
  end

  @doc """
  Terminate parsing and get the final state.

  Sends `:end_document` to the handler.
  """
  @spec terminate(t()) :: {:ok, any()} | {:error, any()}
  def terminate(%__MODULE__{} = partial) do
    cdata_as_chars = Keyword.get(partial.opts, :cdata_as_characters, false)

    try do
      # Process remaining buffered events
      remaining = Native.streaming_finalize(partial.parser)
      saxy_events = EventTransformer.to_saxy_events(remaining)
      state = dispatch_events(saxy_events, partial.handler, partial.state, cdata_as_chars)

      final_state = dispatch_one(partial.handler, :end_document, {}, state)
      {:ok, final_state}
    catch
      {:sax_stop, value} -> {:ok, value}
      {:sax_halt, value} -> {:ok, value}
    end
  end

  @doc """
  Get the current handler state without terminating.
  """
  @spec get_state(t()) :: any()
  def get_state(%__MODULE__{state: state}), do: state

  # -- Private ---------------------------------------------------------------

  @batch_size 500

  defp maybe_send_start_document(%{started: true} = partial), do: partial

  defp maybe_send_start_document(%{started: false} = partial) do
    new_state = dispatch_one(partial.handler, :start_document, [], partial.state)
    %{partial | state: new_state, started: true}
  end

  defp drain_events(partial, cdata_as_chars) do
    case Native.streaming_take_events(partial.parser, @batch_size) do
      [] ->
        partial

      events ->
        saxy_events = EventTransformer.to_saxy_events(events)
        new_state = dispatch_events(saxy_events, partial.handler, partial.state, cdata_as_chars)
        drain_events(%{partial | state: new_state}, cdata_as_chars)
    end
  end

  defp dispatch_events(events, handler, state, cdata_as_chars) do
    Enum.reduce(events, state, fn event, state ->
      {type, data} = normalize_event(event, cdata_as_chars)
      dispatch_one(handler, type, data, state)
    end)
  end

  defp normalize_event({:cdata, content}, true), do: {:characters, content}
  defp normalize_event({type, data}, _), do: {type, data}

  defp dispatch_one(handler, type, data, state) do
    case handler.handle_event(type, data, state) do
      {:ok, new_state} -> new_state
      {:stop, value} -> throw({:sax_stop, value})
      {:halt, value} -> throw({:sax_halt, value})
    end
  end
end
