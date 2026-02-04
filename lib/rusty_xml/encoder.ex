defmodule RustyXML.Encoder do
  @moduledoc false

  @doc """
  Encode an XML element tree to a string. Raises on errors.
  """
  @spec encode!(term(), keyword()) :: String.t()
  def encode!(content, opts \\ []) do
    content
    |> encode_to_iodata(opts)
    |> IO.iodata_to_binary()
  end

  @doc """
  Encode an XML element tree to iodata.
  """
  @spec encode_to_iodata(term(), keyword()) :: iodata()
  def encode_to_iodata(content, opts \\ []) do
    prolog =
      if Keyword.get(opts, :prolog, false) do
        version = Keyword.get(opts, :version, "1.0")
        encoding = Keyword.get(opts, :encoding, "UTF-8")
        [~s(<?xml version="#{version}" encoding="#{encoding}"?>)]
      else
        []
      end

    prolog ++ [encode_content(content)]
  end

  defp encode_content({:element, name, attrs, []}) do
    ["<", name, encode_attributes(attrs), "/>"]
  end

  defp encode_content({:element, name, attrs, children}) do
    [
      "<",
      name,
      encode_attributes(attrs),
      ">",
      Enum.map(children, &encode_content/1),
      "</",
      name,
      ">"
    ]
  end

  defp encode_content({:cdata, content}) do
    ["<![CDATA[", content, "]]>"]
  end

  defp encode_content({:processing_instruction, target, data}) do
    ["<?", target, " ", data, "?>"]
  end

  defp encode_content({:comment, content}) do
    ["<!--", content, "-->"]
  end

  defp encode_content(text) when is_binary(text) do
    escape_text(text)
  end

  defp encode_content(list) when is_list(list) do
    Enum.map(list, &encode_content/1)
  end

  defp encode_attributes([]), do: []

  defp encode_attributes(attrs) do
    Enum.map(attrs, fn {name, value} ->
      [" ", name, "=\"", escape_attribute(value), "\""]
    end)
  end

  defp escape_text(text) do
    text
    |> String.replace("&", "&amp;")
    |> String.replace("<", "&lt;")
    |> String.replace(">", "&gt;")
  end

  defp escape_attribute(value) do
    value
    |> String.replace("&", "&amp;")
    |> String.replace("<", "&lt;")
    |> String.replace(">", "&gt;")
    |> String.replace("\"", "&quot;")
  end
end
