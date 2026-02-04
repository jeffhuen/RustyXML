defmodule RustyXML.Handler do
  @moduledoc """
  Behaviour for SAX event handlers.

  Drop-in replacement for `Saxy.Handler`. Implement `handle_event/3` to process
  XML parsing events.

  ## Event Types

    * `:start_document` — emitted once at the beginning. Data is a keyword list
      with optional `:version`, `:encoding`, and `:standalone` keys.
    * `:start_element` — emitted for each opening tag. Data is `{name, attributes}`
      where attributes is a list of `{name, value}` string tuples.
    * `:characters` — emitted for text content. Data is a binary string.
    * `:cdata` — emitted for CDATA sections. Data is a binary string.
    * `:end_element` — emitted for each closing tag. Data is the element name (binary).
    * `:end_document` — emitted once at the end. Data is `{}`.

  ## Return Values

    * `{:ok, new_state}` — continue parsing with updated state
    * `{:stop, value}` — stop parsing early, return `{:ok, value}`
    * `{:halt, value}` — halt parsing, return `{:halt, value, rest}`

  ## Example

      defmodule CountingHandler do
        @behaviour RustyXML.Handler

        @impl true
        def handle_event(:start_element, _data, count), do: {:ok, count + 1}
        def handle_event(_event, _data, count), do: {:ok, count}
      end

      RustyXML.parse_string("<root><a/><b/></root>", CountingHandler, 0)
      #=> {:ok, 3}

  """

  @type event_type ::
          :start_document
          | :start_element
          | :characters
          | :cdata
          | :end_element
          | :end_document

  @type event_data ::
          keyword()
          | {String.t(), [{String.t(), String.t()}]}
          | String.t()
          | {}

  @callback handle_event(event_type(), event_data(), state) ::
              {:ok, state} | {:stop, any()} | {:halt, any()}
            when state: any()
end
