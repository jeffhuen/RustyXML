defmodule RustyXMLTest do
  use ExUnit.Case, async: true

  import RustyXML

  describe "parse/1" do
    test "parses simple XML" do
      doc = RustyXML.parse("<root>hello</root>")
      assert is_reference(doc)
    end

    test "parses empty element" do
      doc = RustyXML.parse("<root/>")
      assert is_reference(doc)
    end

    test "parses nested elements" do
      doc = RustyXML.parse("<root><child><grandchild/></child></root>")
      assert is_reference(doc)
    end

    test "parses elements with attributes" do
      doc = RustyXML.parse(~s(<div id="main" class="container"/>))
      assert is_reference(doc)
    end

    test "parses CDATA sections" do
      doc = RustyXML.parse("<script><![CDATA[alert('hi')]]></script>")
      assert is_reference(doc)
    end

    test "parses comments" do
      doc = RustyXML.parse("<root><!-- comment --></root>")
      assert is_reference(doc)
    end
  end

  describe "xpath/2 with raw XML" do
    test "returns elements" do
      result = RustyXML.xpath("<root><item/><item/></root>", "//item")
      assert is_list(result)
      assert length(result) == 2
    end

    test "returns text content" do
      result = RustyXML.xpath("<root>hello</root>", "//root/text()")
      # Result should include text
      assert result != nil
    end

    test "handles descendant axis" do
      result = RustyXML.xpath("<a><b><c/></b></a>", "//c")
      assert is_list(result)
      assert length(result) == 1
    end

    test "handles multiple levels" do
      xml = "<root><level1><level2><item>value</item></level2></level1></root>"
      result = RustyXML.xpath(xml, "/root/level1/level2/item")
      assert is_list(result)
      assert length(result) == 1
    end

    test "handles wildcard" do
      result = RustyXML.xpath("<root><a/><b/><c/></root>", "/root/*")
      assert is_list(result)
      assert length(result) == 3
    end

    test "handles predicates" do
      result = RustyXML.xpath("<root><a/><b/><c/></root>", "/root/*[2]")
      assert is_list(result)
      assert length(result) == 1
    end
  end

  describe "xpath/2 with parsed document" do
    test "works with pre-parsed document" do
      doc = RustyXML.parse("<root><a/><b/></root>")
      result = RustyXML.xpath(doc, "//a")
      assert is_list(result)
      assert length(result) == 1
    end

    test "allows multiple queries on same document" do
      doc = RustyXML.parse("<root><a/><b/><c/></root>")

      assert length(RustyXML.xpath(doc, "//a")) == 1
      assert length(RustyXML.xpath(doc, "//b")) == 1
      assert length(RustyXML.xpath(doc, "//c")) == 1
      assert length(RustyXML.xpath(doc, "/root/*")) == 3
    end
  end

  describe "xpath/2 with sigil" do
    test "works with ~x sigil (returns first match without l)" do
      result = RustyXML.xpath("<root><item/></root>", ~x"//item")
      # Without 'l' modifier, returns first match (or nil if none)
      assert result != nil
    end

    test "works with ~x sigil l modifier (returns list)" do
      result = RustyXML.xpath("<root><item/><item/></root>", ~x"//item"l)
      assert is_list(result)
      assert length(result) == 2
    end

    test "returns nil for empty result without o modifier" do
      result = RustyXML.xpath("<root></root>", ~x"//nonexistent")
      assert result == nil
    end

    test "returns string with s modifier" do
      result = RustyXML.xpath("<root><item>hello</item></root>", ~x"//item/text()"s)
      assert result == "hello"
    end

    test "returns integer with i modifier" do
      result = RustyXML.xpath("<root><count>42</count></root>", ~x"//count/text()"i)
      assert result == 42
    end

    test "returns float with f modifier" do
      result = RustyXML.xpath("<root><price>3.14</price></root>", ~x"//price/text()"f)
      assert result == 3.14
    end
  end

  describe "xpath functions" do
    test "count function" do
      result = RustyXML.xpath("<root><a/><b/><c/></root>", "count(/root/*)")
      assert result == 3.0
    end

    test "string-length function" do
      result = RustyXML.xpath("<root>hello</root>", "string-length('hello')")
      assert result == 5.0
    end

    test "concat function" do
      result = RustyXML.xpath("<root/>", "concat('a', 'b', 'c')")
      assert result == "abc"
    end

    test "contains function" do
      result = RustyXML.xpath("<root/>", "contains('hello world', 'world')")
      assert result == true
    end

    test "starts-with function" do
      result = RustyXML.xpath("<root/>", "starts-with('hello', 'hel')")
      assert result == true
    end

    test "substring function" do
      result = RustyXML.xpath("<root/>", "substring('hello', 2, 3)")
      assert result == "ell"
    end

    test "normalize-space function" do
      result = RustyXML.xpath("<root/>", "normalize-space('  hello   world  ')")
      assert result == "hello world"
    end

    test "floor function" do
      result = RustyXML.xpath("<root/>", "floor(3.7)")
      assert result == 3.0
    end

    test "ceiling function" do
      result = RustyXML.xpath("<root/>", "ceiling(3.2)")
      assert result == 4.0
    end

    test "round function" do
      result = RustyXML.xpath("<root/>", "round(3.5)")
      assert result == 4.0
    end

    test "sum function" do
      result = RustyXML.xpath("<root><n>1</n><n>2</n><n>3</n></root>", "sum(/root/n)")
      # sum needs to convert text to numbers
      assert is_number(result)
    end

    test "boolean true" do
      result = RustyXML.xpath("<root/>", "true()")
      assert result == true
    end

    test "boolean false" do
      result = RustyXML.xpath("<root/>", "false()")
      assert result == false
    end

    test "not function" do
      result = RustyXML.xpath("<root/>", "not(false())")
      assert result == true
    end
  end

  describe "~x sigil" do
    test "creates SweetXpath struct" do
      spec = ~x"//item"
      assert %RustyXML.SweetXpath{} = spec
      assert spec.path == "//item"
    end

    test "parses s modifier" do
      spec = ~x"//item"s
      assert spec.cast_to == :string
    end

    test "parses l modifier" do
      spec = ~x"//item"l
      assert spec.is_list == true
    end

    test "parses o modifier" do
      spec = ~x"//item"o
      assert spec.is_optional == true
    end

    test "parses i modifier" do
      spec = ~x"//item"i
      assert spec.cast_to == :integer
    end

    test "parses f modifier" do
      spec = ~x"//item"f
      assert spec.cast_to == :float
    end

    test "parses k modifier" do
      spec = ~x"//item"k
      assert spec.is_keyword == true
    end

    test "parses e modifier (entity/element for chaining)" do
      spec = ~x"//item"e
      # e means return entity (element), not value - is_value should be false
      assert spec.is_value == false
    end

    test "parses multiple modifiers" do
      spec = ~x"//item"slo
      assert spec.cast_to == :string
      assert spec.is_list == true
      assert spec.is_optional == true
    end

    test "parses soft modifiers" do
      spec_s = ~x"//item"S
      assert spec_s.cast_to == :string
      assert spec_s.soft_cast == true

      spec_i = ~x"//item"I
      assert spec_i.cast_to == :integer
      assert spec_i.soft_cast == true

      spec_f = ~x"//item"F
      assert spec_f.cast_to == :float
      assert spec_f.soft_cast == true
    end
  end

  describe "xmap/2" do
    test "extracts multiple values" do
      xml = "<root><a>1</a><b>2</b></root>"

      result =
        RustyXML.xmap(xml,
          a: ~x"//a",
          b: ~x"//b"
        )

      assert is_map(result)
      assert Map.has_key?(result, :a)
      assert Map.has_key?(result, :b)
    end

    test "works with parsed document" do
      doc = RustyXML.parse("<root><x>foo</x><y>bar</y></root>")

      result =
        RustyXML.xmap(doc,
          x: ~x"//x",
          y: ~x"//y"
        )

      assert is_map(result)
      assert Map.has_key?(result, :x)
      assert Map.has_key?(result, :y)
    end
  end

  describe "root/1" do
    test "returns root element" do
      doc = RustyXML.parse("<root><child/></root>")
      root = RustyXML.root(doc)
      assert root != nil
    end

    test "returns tuple with element info" do
      doc = RustyXML.parse("<myroot attr=\"val\"><child/></myroot>")
      root = RustyXML.root(doc)
      assert is_tuple(root)
      assert elem(root, 0) == :element
    end
  end

  # ==========================================================================
  # Lazy XPath + Batch Accessor Clamping
  # ==========================================================================

  describe "Native.xpath_lazy/2 and result accessors" do
    setup do
      xml = "<root><item id=\"1\">A</item><item id=\"2\">B</item><item id=\"3\">C</item></root>"
      doc = RustyXML.Native.parse(xml)
      result = RustyXML.Native.xpath_lazy(doc, "//item")
      %{result: result}
    end

    test "result_count returns correct count", %{result: result} do
      assert RustyXML.Native.result_count(result) == 3
    end

    test "result_text returns text for valid index", %{result: result} do
      assert RustyXML.Native.result_text(result, 0) == "A"
      assert RustyXML.Native.result_text(result, 2) == "C"
    end

    test "result_text returns nil for out-of-range index", %{result: result} do
      assert RustyXML.Native.result_text(result, 99) == nil
    end

    test "result_attr returns attribute value", %{result: result} do
      assert RustyXML.Native.result_attr(result, 0, "id") == "1"
      assert RustyXML.Native.result_attr(result, 2, "id") == "3"
    end

    test "result_name returns element name", %{result: result} do
      assert RustyXML.Native.result_name(result, 0) == "item"
    end

    test "result_node returns full element", %{result: result} do
      node = RustyXML.Native.result_node(result, 0)
      assert {:element, "item", _attrs, _children} = node
    end
  end

  describe "batch accessor clamping" do
    # Compute usize::MAX portably (works on both 32-bit and 64-bit BEAM)
    # Compute usize::MAX portably — :wordsize returns bytes (4 or 8)
    @usize_max Bitwise.bsl(1, :erlang.system_info(:wordsize) * 8) - 1

    setup do
      xml = "<r><i id=\"a\">X</i><i id=\"b\">Y</i><i id=\"c\">Z</i></r>"
      doc = RustyXML.Native.parse(xml)
      result = RustyXML.Native.xpath_lazy(doc, "//i")
      %{result: result}
    end

    # -- result_texts/3 --

    test "result_texts returns all items for exact range", %{result: result} do
      texts = RustyXML.Native.result_texts(result, 0, 3)
      assert texts == ["X", "Y", "Z"]
    end

    test "result_texts clamps when count exceeds results", %{result: result} do
      texts = RustyXML.Native.result_texts(result, 0, 1_000_000)
      assert texts == ["X", "Y", "Z"]
    end

    test "result_texts returns subset from offset", %{result: result} do
      texts = RustyXML.Native.result_texts(result, 1, 100)
      assert texts == ["Y", "Z"]
    end

    test "result_texts returns empty when start beyond range", %{result: result} do
      texts = RustyXML.Native.result_texts(result, 99, 10)
      assert texts == []
    end

    test "result_texts handles usize_max count without hanging", %{result: result} do
      # This would previously iterate billions of times; now clamps to 3
      texts = RustyXML.Native.result_texts(result, 0, @usize_max)
      assert texts == ["X", "Y", "Z"]
    end

    # -- result_attrs/4 --

    test "result_attrs returns all attrs for exact range", %{result: result} do
      ids = RustyXML.Native.result_attrs(result, "id", 0, 3)
      assert ids == ["a", "b", "c"]
    end

    test "result_attrs clamps when count exceeds results", %{result: result} do
      ids = RustyXML.Native.result_attrs(result, "id", 0, 1_000_000)
      assert ids == ["a", "b", "c"]
    end

    test "result_attrs returns subset from offset", %{result: result} do
      ids = RustyXML.Native.result_attrs(result, "id", 2, 100)
      assert ids == ["c"]
    end

    test "result_attrs returns empty when start beyond range", %{result: result} do
      ids = RustyXML.Native.result_attrs(result, "id", 99, 10)
      assert ids == []
    end

    test "result_attrs handles usize_max count without hanging", %{result: result} do
      ids = RustyXML.Native.result_attrs(result, "id", 0, @usize_max)
      assert ids == ["a", "b", "c"]
    end

    # -- result_extract/5 --

    test "result_extract returns all maps for exact range", %{result: result} do
      maps = RustyXML.Native.result_extract(result, 0, 3, ["id"], true)
      assert length(maps) == 3
      assert Enum.at(maps, 0)[:name] == "i"
      assert Enum.at(maps, 0)[:text] == "X"
      assert Enum.at(maps, 0)["id"] == "a"
    end

    test "result_extract clamps when count exceeds results", %{result: result} do
      maps = RustyXML.Native.result_extract(result, 0, 1_000_000, ["id"], false)
      assert length(maps) == 3
    end

    test "result_extract returns subset from offset", %{result: result} do
      maps = RustyXML.Native.result_extract(result, 2, 100, ["id"], false)
      assert length(maps) == 1
      assert Enum.at(maps, 0)["id"] == "c"
    end

    test "result_extract returns empty when start beyond range", %{result: result} do
      maps = RustyXML.Native.result_extract(result, 99, 10, ["id"], false)
      assert maps == []
    end

    test "result_extract handles usize_max count without hanging", %{result: result} do
      maps = RustyXML.Native.result_extract(result, 0, @usize_max, ["id"], true)
      assert length(maps) == 3
    end
  end

  describe "Native.parse_events/1" do
    test "returns list of events" do
      events = RustyXML.Native.parse_events("<root>text</root>")
      assert is_list(events)
      assert length(events) >= 3
    end

    test "includes start, text, and end events" do
      events = RustyXML.Native.parse_events("<r>t</r>")

      assert Enum.any?(events, fn e -> match?({:start_element, _, _}, e) end)
      assert Enum.any?(events, fn e -> match?({:text, _}, e) end)
      assert Enum.any?(events, fn e -> match?({:end_element, _}, e) end)
    end

    test "handles empty elements" do
      events = RustyXML.Native.parse_events("<br/>")
      assert Enum.any?(events, fn e -> match?({:empty_element, _, _}, e) end)
    end

    test "handles attributes" do
      events = RustyXML.Native.parse_events(~s(<div id="x"/>))
      [{:empty_element, "div", attrs}] = events
      assert is_list(attrs)
    end

    test "handles CDATA" do
      events = RustyXML.Native.parse_events("<x><![CDATA[data]]></x>")
      assert Enum.any?(events, fn e -> match?({:cdata, _}, e) end)
    end

    test "handles comments" do
      events = RustyXML.Native.parse_events("<x><!-- comment --></x>")
      assert Enum.any?(events, fn e -> match?({:comment, _}, e) end)
    end
  end
end
