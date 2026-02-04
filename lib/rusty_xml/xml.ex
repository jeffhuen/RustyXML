defmodule RustyXML.XML do
  @moduledoc """
  XML building DSL for constructing XML documents programmatically.

  Drop-in replacement for `Saxy.XML`.

  ## Example

      import RustyXML.XML

      doc = element("person", [{"id", "1"}], [
        element("name", [], ["John Doe"]),
        element("email", [], ["john@example.com"])
      ])

      RustyXML.encode!(doc)
      #=> "<person id=\\"1\\"><name>John Doe</name><email>john@example.com</email></person>"

  """

  @type content :: String.t() | {:cdata, String.t()} | element()
  @type element :: {:element, String.t(), [{String.t(), String.t()}], [content()]}

  @doc """
  Create an XML element with children.
  """
  @spec element(String.t(), [{String.t(), String.t()}], [content()]) :: element()
  def element(name, attributes, children) do
    {:element, name, attributes, List.wrap(children)}
  end

  @doc """
  Create an empty (self-closing) XML element.
  """
  @spec empty_element(String.t(), [{String.t(), String.t()}]) :: element()
  def empty_element(name, attributes \\ []) do
    {:element, name, attributes, []}
  end

  @doc """
  Create a text content node.
  """
  @spec characters(String.t()) :: String.t()
  def characters(text) when is_binary(text), do: text

  @doc """
  Create a CDATA section.
  """
  @spec cdata(String.t()) :: {:cdata, String.t()}
  def cdata(content) when is_binary(content), do: {:cdata, content}

  @doc """
  Create a processing instruction.
  """
  @spec processing_instruction(String.t(), String.t()) ::
          {:processing_instruction, String.t(), String.t()}
  def processing_instruction(target, data) do
    {:processing_instruction, target, data}
  end
end
