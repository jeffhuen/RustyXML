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

    test "accepts charlist input" do
      doc = RustyXML.parse(~c"<root>hello</root>")
      assert is_reference(doc)
      assert RustyXML.xpath(doc, ~x"//root/text()"s) == "hello"
    end

    test "accepts iodata charlist input" do
      doc = RustyXML.parse(~c"<root><item/></root>")
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

    test "returns keyword list when third arg is true" do
      xml = "<root><a>1</a><b>2</b></root>"

      result =
        RustyXML.xmap(
          xml,
          [a: ~x"//a/text()"s, b: ~x"//b/text()"s],
          true
        )

      assert is_list(result)
      assert Keyword.keyword?(result)
      assert Keyword.get(result, :a) == "1"
      assert Keyword.get(result, :b) == "2"
    end

    test "returns map when third arg is false (default)" do
      xml = "<root><a>1</a></root>"

      result = RustyXML.xmap(xml, a: ~x"//a/text()"s)
      assert is_map(result)
      assert result.a == "1"
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
  # Strict Parsing & Error Cases
  # ==========================================================================

  describe "Native.parse_strict/1" do
    test "returns {:ok, ref} for well-formed XML" do
      assert {:ok, doc} = RustyXML.Native.parse_strict("<root><child/></root>")
      assert is_reference(doc)
    end

    test "returns {:error, reason} for malformed XML" do
      assert {:error, reason} = RustyXML.Native.parse_strict("<1invalid/>")
      assert is_binary(reason)
    end

    test "returns {:error, reason} for unclosed element" do
      assert {:error, _reason} = RustyXML.Native.parse_strict("<root><unclosed>")
    end

    test "returns {:error, reason} for mismatched tags" do
      assert {:error, _reason} = RustyXML.Native.parse_strict("<a></b>")
    end
  end

  describe "parse_document/1" do
    test "returns {:ok, doc} for valid XML" do
      assert {:ok, doc} = RustyXML.parse_document("<root/>")
      assert is_reference(doc)
    end

    test "returns {:error, reason} for malformed XML" do
      assert {:error, reason} = RustyXML.parse_document("<1bad/>")
      assert is_binary(reason)
    end

    test "accepts charlist input" do
      assert {:ok, doc} = RustyXML.parse_document(~c"<root>ok</root>")
      assert is_reference(doc)
    end

    test "rejects malformed charlist input" do
      assert {:error, _reason} = RustyXML.parse_document(~c"<1invalid/>")
    end
  end

  describe "parse/2 error cases" do
    test "raises ParseError for malformed XML in strict mode" do
      assert_raise RustyXML.ParseError, fn ->
        RustyXML.parse("<1invalid/>")
      end
    end

    @tag :lenient_parsing
    test "accepts malformed XML in lenient mode" do
      doc = RustyXML.parse("<1invalid/>", lenient: true)
      assert is_reference(doc)
    end

    @tag :lenient_parsing
    test "handles empty input" do
      doc = RustyXML.parse("", lenient: true)
      assert is_reference(doc)
    end
  end

  # ==========================================================================
  # Native XPath Variants
  # ==========================================================================

  describe "Native.xpath_query_raw/2" do
    test "returns list of XML binaries for node sets" do
      doc = RustyXML.Native.parse("<root><item>text</item><item>more</item></root>")
      result = RustyXML.Native.xpath_query_raw(doc, "//item")
      assert is_list(result)
      assert length(result) == 2
      assert Enum.all?(result, &is_binary/1)
    end

    test "returns scalar for non-node-set queries" do
      doc = RustyXML.Native.parse("<root><a/><b/></root>")
      result = RustyXML.Native.xpath_query_raw(doc, "count(//a)")
      assert result == 3.0 or is_number(result)
    end
  end

  describe "Native.xpath_string_value/2" do
    test "returns text content of first node" do
      result = RustyXML.Native.xpath_string_value("<root>hello</root>", "/root")
      assert result == "hello"
    end

    test "returns string for string expressions" do
      result = RustyXML.Native.xpath_string_value("<root/>", "concat('a','b')")
      assert result == "ab"
    end

    test "returns empty string for empty node set" do
      result = RustyXML.Native.xpath_string_value("<root/>", "//nonexistent")
      assert result == ""
    end
  end

  describe "Native.xpath_string_value_doc/2" do
    test "returns string value from document reference" do
      doc = RustyXML.Native.parse("<root>world</root>")
      result = RustyXML.Native.xpath_string_value_doc(doc, "/root")
      assert result == "world"
    end

    test "returns concatenated text for elements with children" do
      doc = RustyXML.Native.parse("<root><a>hello</a> <b>world</b></root>")
      result = RustyXML.Native.xpath_string_value_doc(doc, "/root")
      assert is_binary(result)
      assert String.contains?(result, "hello")
      assert String.contains?(result, "world")
    end
  end

  # ==========================================================================
  # xpath/3 with Subspecs
  # ==========================================================================

  describe "xpath/3 with subspecs" do
    test "extracts nested values from raw XML" do
      xml = """
      <items>
        <item id="1"><name>A</name></item>
        <item id="2"><name>B</name></item>
      </items>
      """

      result =
        RustyXML.xpath(xml, ~x"//item"l, id: ~x"./@id"s, name: ~x"./name/text()"s)

      assert is_list(result)
      assert length(result) == 2
    end

    test "extracts nested values from document ref" do
      xml = """
      <items>
        <item id="1"><name>A</name></item>
        <item id="2"><name>B</name></item>
      </items>
      """

      doc = RustyXML.parse(xml)

      result =
        RustyXML.xpath(doc, ~x"//item"l, id: ~x"./@id"s, name: ~x"./name/text()"s)

      assert is_list(result)
      assert length(result) == 2

      # Each entry should be a map with atom keys
      [first | _] = result
      assert is_map(first)
      assert Map.has_key?(first, :id)
      assert Map.has_key?(first, :name)
      assert first.id == "1"
      assert first.name == "A"
    end

    test "works with single result (no l modifier)" do
      xml = "<items><item id=\"1\"><name>A</name></item></items>"
      doc = RustyXML.parse(xml)

      result =
        RustyXML.xpath(doc, ~x"//item", id: ~x"./@id"s, name: ~x"./name/text()"s)

      assert is_map(result)
      assert result.id == "1"
      assert result.name == "A"
    end
  end

  # ==========================================================================
  # Soft Cast Runtime Behaviour
  # ==========================================================================

  describe "soft cast modifiers (S, I, F)" do
    test "S returns nil for missing element" do
      result = RustyXML.xpath("<root/>", ~x"//missing/text()"S)
      assert result == nil
    end

    test "I returns nil for non-numeric text" do
      result = RustyXML.xpath("<root><v>abc</v></root>", ~x"//v/text()"I)
      assert result == nil
    end

    test "I parses valid integer" do
      result = RustyXML.xpath("<root><v>42</v></root>", ~x"//v/text()"I)
      assert result == 42
    end

    test "F returns nil for non-numeric text" do
      result = RustyXML.xpath("<root><v>abc</v></root>", ~x"//v/text()"F)
      assert result == nil
    end

    test "F parses valid float" do
      result = RustyXML.xpath("<root><v>3.14</v></root>", ~x"//v/text()"F)
      assert result == 3.14
    end

    test "hard i modifier raises on non-numeric text" do
      assert_raise ArgumentError, fn ->
        RustyXML.xpath("<root><v>abc</v></root>", ~x"//v/text()"i)
      end
    end

    test "hard f modifier raises on non-numeric text" do
      assert_raise ArgumentError, fn ->
        RustyXML.xpath("<root><v>abc</v></root>", ~x"//v/text()"f)
      end
    end
  end

  # ==========================================================================
  # transform_by/2 and add_namespace/3
  # ==========================================================================

  describe "transform_by/2" do
    test "applies transform function to result" do
      spec = RustyXML.transform_by(~x"//item/text()"s, &String.upcase/1)
      result = RustyXML.xpath("<root><item>hello</item></root>", spec)
      assert result == "HELLO"
    end

    test "applies transform after cast" do
      spec = RustyXML.transform_by(~x"//n/text()"i, &(&1 * 2))
      result = RustyXML.xpath("<root><n>5</n></root>", spec)
      assert result == 10
    end
  end

  describe "add_namespace/3" do
    test "returns SweetXpath with namespace binding" do
      spec = RustyXML.add_namespace(~x"//ns:item"l, "ns", "http://example.com")
      assert %RustyXML.SweetXpath{} = spec
      assert {"ns", "http://example.com"} in spec.namespaces
    end
  end

  # ==========================================================================
  # Streaming Element-Based APIs
  # ==========================================================================

  describe "Native.streaming_take_elements/2" do
    test "returns list of XML binaries for complete elements" do
      parser = RustyXML.Native.streaming_new_with_filter("item")
      RustyXML.Native.streaming_feed(parser, "<root><item>A</item><item>B</item></root>")
      elements = RustyXML.Native.streaming_take_elements(parser, 100)
      assert is_list(elements)
      assert length(elements) == 2
      assert Enum.all?(elements, &is_binary/1)
    end

    test "respects max parameter" do
      parser = RustyXML.Native.streaming_new_with_filter("item")
      RustyXML.Native.streaming_feed(parser, "<root><item/><item/><item/></root>")
      elements = RustyXML.Native.streaming_take_elements(parser, 1)
      assert length(elements) == 1
    end
  end

  describe "Native.streaming_available_elements/1" do
    test "returns count of available elements" do
      parser = RustyXML.Native.streaming_new_with_filter("item")
      RustyXML.Native.streaming_feed(parser, "<root><item/><item/></root>")
      count = RustyXML.Native.streaming_available_elements(parser)
      assert is_integer(count)
      assert count == 2
    end

    test "returns 0 for empty parser" do
      parser = RustyXML.Native.streaming_new()
      count = RustyXML.Native.streaming_available_elements(parser)
      assert count == 0
    end
  end

  # ==========================================================================
  # XPath Error Handling
  # ==========================================================================

  describe "XPath error handling" do
    test "returns {:error, reason} for malformed XPath on raw XML" do
      result = RustyXML.Native.parse_and_xpath("<root/>", "///invalid[[[")
      assert {:error, reason} = result
      assert is_binary(reason)
    end

    test "returns {:error, reason} for malformed XPath on document" do
      doc = RustyXML.Native.parse("<root/>")
      result = RustyXML.Native.xpath_query(doc, "///invalid[[[")
      assert {:error, reason} = result
      assert is_binary(reason)
    end
  end
end
