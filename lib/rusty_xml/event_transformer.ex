defmodule RustyXML.EventTransformer do
  @moduledoc false

  @doc """
  Transform RustyXML native SAX events to Saxy-compatible format.

  Native events from `sax_parse/1` and streaming parser use a different
  tuple format than Saxy. This module bridges the gap.
  """
  @spec to_saxy_events([tuple()]) :: [{atom(), any()}]
  def to_saxy_events(events) do
    List.foldr(events, [], fn event, acc ->
      transform_event(event, acc)
    end)
  end

  defp transform_event({:start_element, name, attrs}, acc) do
    [{:start_element, {name, attrs}} | acc]
  end

  defp transform_event({:end_element, name}, acc) do
    [{:end_element, name} | acc]
  end

  defp transform_event({:empty_element, name, attrs}, acc) do
    [{:start_element, {name, attrs}}, {:end_element, name} | acc]
  end

  defp transform_event({:text, content}, acc) do
    [{:characters, content} | acc]
  end

  defp transform_event({:cdata, content}, acc) do
    [{:cdata, content} | acc]
  end

  # Comments and PIs are not emitted by Saxy — drop them
  defp transform_event({:comment, _}, acc), do: acc
  defp transform_event({:processing_instruction, _, _}, acc), do: acc
  defp transform_event(_, acc), do: acc
end
