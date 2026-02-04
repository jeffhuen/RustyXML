defprotocol RustyXML.Builder do
  @moduledoc """
  Protocol for converting Elixir data structures to XML.

  Drop-in replacement for `Saxy.Builder`.

  ## Manual Implementation

      defimpl RustyXML.Builder, for: MyStruct do
        import RustyXML.XML

        def build(%MyStruct{name: name, id: id}) do
          element("person", [{"id", to_string(id)}], [
            element("name", [], [name])
          ])
        end
      end

  """

  @doc """
  Convert a term to an XML element structure suitable for `RustyXML.encode!/1`.
  """
  @spec build(term()) :: RustyXML.XML.element() | String.t() | [RustyXML.XML.element() | String.t()]
  def build(term)
end

defimpl RustyXML.Builder, for: BitString do
  def build(string) when is_binary(string), do: string
end

defimpl RustyXML.Builder, for: Tuple do
  def build({:element, _, _, _} = element), do: element
  def build({:cdata, _} = cdata), do: cdata
  def build({:processing_instruction, _, _} = pi), do: pi
end

defimpl RustyXML.Builder, for: List do
  def build(list), do: Enum.map(list, &RustyXML.Builder.build/1)
end
