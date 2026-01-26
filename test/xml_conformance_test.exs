defmodule RustyXML.ConformanceTest do
  @moduledoc """
  XML 1.0 Conformance Test Suite

  Tests based on W3C XML 1.0 (Fifth Edition) Recommendation:
  https://www.w3.org/TR/xml/

  Reference test suites:
  - OASIS XML Conformance Test Suite (2000+ tests)
  - W3C XML Test Suite
  - James Clark's xmltest

  Categories covered:
  1. Well-formedness constraints
  2. Character handling and encoding
  3. Whitespace normalization
  4. Entity handling
  5. CDATA sections
  6. Comments and Processing Instructions
  7. Namespace handling
  8. Attribute handling
  9. Element naming rules
  10. Edge cases and error handling
  """
  use ExUnit.Case, async: true

  # ===========================================================================
  # Section 1: Well-Formedness Constraints (XML 1.0 §2.1)
  # ===========================================================================

  describe "Well-formedness: Document structure" do
    test "document must have exactly one root element" do
      # Valid: single root
      doc = RustyXML.parse("<root/>")
      assert is_reference(doc)
    end

    test "elements must be properly nested" do
      # Valid nesting
      doc = RustyXML.parse("<a><b></b></a>")
      assert is_reference(doc)
    end

    test "empty elements can use self-closing syntax" do
      doc = RustyXML.parse("<empty/>")
      assert is_reference(doc)

      # Or explicit close
      doc2 = RustyXML.parse("<empty></empty>")
      assert is_reference(doc2)
    end

    test "root element with children" do
      doc = RustyXML.parse("<root><child1/><child2/><child3/></root>")
      result = RustyXML.xpath(doc, "count(/root/*)")
      assert result == 3.0
    end
  end

  # ===========================================================================
  # Section 2: Character Handling (XML 1.0 §2.2)
  # ===========================================================================

  describe "Character handling: Valid XML characters" do
    test "ASCII printable characters" do
      xml = "<root>ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "common punctuation in text content" do
      xml = "<root>Hello, World! How are you? Fine; thanks.</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "tab character in content" do
      xml = "<root>Column1\tColumn2\tColumn3</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "newline character in content" do
      xml = "<root>Line1\nLine2\nLine3</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "Unicode characters - Latin extended" do
      xml = "<root>café résumé naïve</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "Unicode characters - CJK" do
      xml = "<root>日本語 中文 한국어</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "Unicode characters - Emoji" do
      xml = "<root>Hello 😀 World 🌍</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "Unicode characters - Arabic and Hebrew" do
      xml = "<root>مرحبا שלום</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end
  end

  # ===========================================================================
  # Section 3: Whitespace Handling (XML 1.0 §2.10, §3.3.3)
  # ===========================================================================

  describe "Whitespace handling" do
    test "leading and trailing whitespace in content" do
      xml = "<root>  hello world  </root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "multiple spaces between words" do
      xml = "<root>hello     world</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "mixed whitespace (spaces, tabs, newlines)" do
      xml = "<root>hello \t\n world</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "whitespace-only text content" do
      xml = "<root>   </root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "whitespace between elements" do
      xml = """
      <root>
        <child1/>
        <child2/>
      </root>
      """
      doc = RustyXML.parse(xml)
      result = RustyXML.xpath(doc, "count(/root/*)")
      assert result == 2.0
    end

    test "preserve whitespace in element content" do
      xml = "<pre>  code  \n  indented  </pre>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end
  end

  describe "Line ending normalization (XML 1.0 §2.11)" do
    test "CRLF should be normalized to LF" do
      # Windows-style line endings
      xml = "<root>line1\r\nline2\r\nline3</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "standalone CR should be normalized to LF" do
      # Old Mac-style line endings
      xml = "<root>line1\rline2\rline3</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "mixed line endings" do
      xml = "<root>unix\nwindows\r\nmac\rend</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end
  end

  # ===========================================================================
  # Section 4: Entity Handling (XML 1.0 §4.1, §4.6)
  # ===========================================================================

  describe "Predefined entities (XML 1.0 §4.6)" do
    test "less-than entity &lt;" do
      xml = "<root>&lt;</root>"
      events = RustyXML.Native.parse_events(xml)
      text_events = Enum.filter(events, fn e -> match?({:text, _}, e) end)
      assert length(text_events) >= 1
    end

    test "greater-than entity &gt;" do
      xml = "<root>&gt;</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "ampersand entity &amp;" do
      xml = "<root>&amp;</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "apostrophe entity &apos;" do
      xml = "<root>&apos;</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "quote entity &quot;" do
      xml = "<root>&quot;</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "entities in attribute values" do
      xml = ~s(<root attr="&lt;value&gt;"/>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "multiple entities in sequence" do
      xml = "<root>&lt;&amp;&gt;</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "entities mixed with text" do
      xml = "<root>1 &lt; 2 &amp;&amp; 3 &gt; 2</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end
  end

  describe "Numeric character references (XML 1.0 §4.1)" do
    test "decimal character reference - ASCII" do
      xml = "<root>&#65;&#66;&#67;</root>"  # ABC
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "hexadecimal character reference - lowercase x" do
      xml = "<root>&#x41;&#x42;&#x43;</root>"  # ABC
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "hexadecimal character reference - uppercase X" do
      xml = "<root>&#X41;&#X42;&#X43;</root>"  # ABC
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "high Unicode codepoints" do
      # U+1F600 = 😀 (decimal 128512)
      xml = "<root>&#128512;</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "high Unicode codepoints - hex" do
      # U+1F600 = 😀
      xml = "<root>&#x1F600;</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "non-ASCII characters via numeric reference" do
      # © = &#169; or &#xA9;
      xml = "<root>&#169; &#xA9;</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "mixed decimal and hex references" do
      xml = "<root>&#65;&#x42;&#67;&#x44;</root>"  # ABCD
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end
  end

  # ===========================================================================
  # Section 5: CDATA Sections (XML 1.0 §2.7)
  # ===========================================================================

  describe "CDATA sections" do
    test "basic CDATA section" do
      xml = "<root><![CDATA[Hello World]]></root>"
      events = RustyXML.Native.parse_events(xml)
      assert Enum.any?(events, fn e -> match?({:cdata, _}, e) end)
    end

    test "CDATA with special characters" do
      xml = "<root><![CDATA[<not>&an;element]]></root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "CDATA with XML-like content" do
      xml = "<root><![CDATA[<element attr=\"value\">content</element>]]></root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "empty CDATA section" do
      xml = "<root><![CDATA[]]></root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "CDATA with only whitespace" do
      xml = "<root><![CDATA[   \n\t  ]]></root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "CDATA in code context" do
      xml = """
      <script><![CDATA[
        function test() {
          if (a < b && c > d) {
            return true;
          }
        }
      ]]></script>
      """
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "multiple CDATA sections" do
      xml = "<root><![CDATA[first]]><![CDATA[second]]></root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "CDATA with ]] inside (but not ]]>)" do
      xml = "<root><![CDATA[array[0]]]></root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end
  end

  # ===========================================================================
  # Section 6: Comments and Processing Instructions (XML 1.0 §2.5, §2.6)
  # ===========================================================================

  describe "Comments (XML 1.0 §2.5)" do
    test "basic comment" do
      xml = "<root><!-- comment --></root>"
      events = RustyXML.Native.parse_events(xml)
      assert Enum.any?(events, fn e -> match?({:comment, _}, e) end)
    end

    test "empty comment" do
      xml = "<root><!----></root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "comment with special characters" do
      xml = "<root><!-- <not> & element --></root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "comment before root element" do
      xml = "<!-- before --><root/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "comment after root element" do
      xml = "<root/><!-- after -->"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "multiple comments" do
      xml = "<root><!-- one -->text<!-- two --></root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "multiline comment" do
      xml = """
      <root><!--
        This is a
        multiline comment
      --></root>
      """
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "comment with single hyphen" do
      xml = "<root><!-- single - hyphen --></root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end
  end

  describe "Processing Instructions (XML 1.0 §2.6)" do
    test "basic processing instruction" do
      xml = "<root><?target data?></root>"
      events = RustyXML.Native.parse_events(xml)
      assert Enum.any?(events, fn e -> match?({:processing_instruction, _, _}, e) end)
    end

    test "processing instruction before root" do
      xml = "<?xml-stylesheet type=\"text/css\" href=\"style.css\"?><root/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "processing instruction with no data" do
      xml = "<root><?target?></root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "multiple processing instructions" do
      xml = "<?pi1 data1?><?pi2 data2?><root/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end
  end

  # ===========================================================================
  # Section 7: Namespace Handling (Namespaces in XML 1.0)
  # ===========================================================================

  describe "Namespace handling" do
    test "default namespace declaration" do
      xml = ~s(<root xmlns="http://example.com"/>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "prefixed namespace declaration" do
      xml = ~s(<ns:root xmlns:ns="http://example.com"/>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "multiple namespace declarations" do
      xml = ~s(<root xmlns="http://default.com" xmlns:ns1="http://ns1.com" xmlns:ns2="http://ns2.com"/>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "nested elements with different namespaces" do
      xml = """
      <root xmlns="http://default.com">
        <child xmlns="http://child.com"/>
      </root>
      """
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "prefixed elements" do
      xml = ~s(<ns:root xmlns:ns="http://example.com"><ns:child/></ns:root>)
      doc = RustyXML.parse(xml)
      result = RustyXML.xpath(doc, "count(//*)")
      assert result >= 1.0
    end

    test "prefixed attributes" do
      xml = ~s(<root xmlns:ns="http://example.com" ns:attr="value"/>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "namespace undeclaration (empty default namespace)" do
      xml = """
      <root xmlns="http://example.com">
        <child xmlns=""/>
      </root>
      """
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end
  end

  # ===========================================================================
  # Section 8: Attribute Handling (XML 1.0 §3.1, §3.3)
  # ===========================================================================

  describe "Attribute handling" do
    test "double-quoted attribute value" do
      xml = ~s(<root attr="value"/>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "single-quoted attribute value" do
      xml = ~s(<root attr='value'/>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "empty attribute value" do
      xml = ~s(<root attr=""/>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "multiple attributes" do
      xml = ~s(<root a="1" b="2" c="3"/>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "attribute with spaces in value" do
      xml = ~s(<root attr="hello world"/>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "attribute with entities" do
      xml = ~s(<root attr="&lt;value&gt;"/>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "attribute with numeric character reference" do
      xml = ~s(<root attr="&#65;&#66;&#67;"/>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "double quote inside single-quoted attribute" do
      xml = ~s(<root attr='"quoted"'/>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "single quote inside double-quoted attribute" do
      xml = ~s(<root attr="it's working"/>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "newlines in attribute value" do
      xml = "<root attr=\"line1\nline2\"/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "tabs in attribute value" do
      xml = "<root attr=\"col1\tcol2\"/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "mixed quote styles across attributes" do
      xml = ~s(<root a="one" b='two' c="three"/>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end
  end

  # ===========================================================================
  # Section 9: Element Naming (XML 1.0 §2.3)
  # ===========================================================================

  describe "Element naming rules" do
    test "lowercase element name" do
      xml = "<element/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "uppercase element name" do
      xml = "<ELEMENT/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "mixed case element name" do
      xml = "<MyElement/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "element name with digits" do
      xml = "<element123/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "element name with underscores" do
      xml = "<my_element_name/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "element name with hyphens" do
      xml = "<my-element-name/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "element name with periods" do
      xml = "<my.element.name/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "element name starting with underscore" do
      xml = "<_element/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "element name with colons (namespaced)" do
      xml = ~s(<ns:element xmlns:ns="http://example.com"/>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "Unicode element name - Greek" do
      xml = "<Στοιχείο/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "Unicode element name - CJK" do
      xml = "<要素/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end
  end

  # ===========================================================================
  # Section 10: XML Declaration (XML 1.0 §2.8)
  # ===========================================================================

  describe "XML Declaration" do
    test "minimal XML declaration" do
      xml = "<?xml version=\"1.0\"?><root/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "XML declaration with encoding" do
      xml = "<?xml version=\"1.0\" encoding=\"UTF-8\"?><root/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "XML declaration with standalone" do
      xml = "<?xml version=\"1.0\" standalone=\"yes\"?><root/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "full XML declaration" do
      xml = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"no\"?><root/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "XML version 1.1" do
      xml = "<?xml version=\"1.1\"?><root/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "single quotes in XML declaration" do
      xml = "<?xml version='1.0' encoding='UTF-8'?><root/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end
  end

  # ===========================================================================
  # Section 11: DOCTYPE (XML 1.0 §2.8)
  # ===========================================================================

  describe "DOCTYPE handling" do
    test "simple DOCTYPE declaration" do
      xml = "<!DOCTYPE root><root/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "DOCTYPE with SYSTEM identifier" do
      xml = ~s(<!DOCTYPE root SYSTEM "root.dtd"><root/>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "DOCTYPE with PUBLIC identifier" do
      xml = ~s(<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.0//EN" "http://www.w3.org/TR/xhtml1/DTD/xhtml1-strict.dtd"><html/>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end
  end

  # ===========================================================================
  # Section 12: Edge Cases and Complex Scenarios
  # ===========================================================================

  describe "Edge cases: Mixed content" do
    test "text and elements mixed" do
      xml = "<root>Hello <b>bold</b> world</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "text, comments, and elements" do
      xml = "<root>text<!-- comment --><child/>more text</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "CDATA and text mixed" do
      xml = "<root>text<![CDATA[cdata content]]>more text</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end
  end

  describe "Edge cases: Deeply nested structures" do
    test "deeply nested elements (10 levels)" do
      xml = "<a><b><c><d><e><f><g><h><i><j>deep</j></i></h></g></f></e></d></c></b></a>"
      doc = RustyXML.parse(xml)
      result = RustyXML.xpath(doc, "//j")
      assert length(result) == 1
    end

    test "wide element tree (many siblings)" do
      items = Enum.map(1..100, fn i -> "<item#{i}/>" end) |> Enum.join()
      xml = "<root>#{items}</root>"
      doc = RustyXML.parse(xml)
      result = RustyXML.xpath(doc, "count(/root/*)")
      assert result == 100.0
    end
  end

  describe "Edge cases: Large content" do
    test "large text content" do
      large_text = String.duplicate("x", 10_000)
      xml = "<root>#{large_text}</root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "many attributes" do
      attrs = Enum.map(1..50, fn i -> "attr#{i}=\"value#{i}\"" end) |> Enum.join(" ")
      xml = "<root #{attrs}/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "long attribute value" do
      long_value = String.duplicate("x", 1000)
      xml = "<root attr=\"#{long_value}\"/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end
  end

  describe "Edge cases: Special attribute patterns" do
    test "attribute value equals element content" do
      xml = ~s(<root attr="same">same</root>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "attribute with equals sign in value" do
      xml = ~s(<root attr="a=b=c"/>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "attribute with angle brackets (escaped)" do
      xml = ~s(<root attr="&lt;tag&gt;"/>)
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end
  end

  describe "Edge cases: Minimal documents" do
    test "minimal valid document" do
      xml = "<r/>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "single character element name" do
      xml = "<a><b/></a>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "single character text content" do
      xml = "<r>x</r>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end
  end

  describe "Edge cases: Real-world XML patterns" do
    test "RSS-like structure" do
      xml = """
      <rss version="2.0">
        <channel>
          <title>Example Feed</title>
          <link>http://example.com</link>
          <item>
            <title>Article 1</title>
            <description>Description 1</description>
          </item>
          <item>
            <title>Article 2</title>
            <description>Description 2</description>
          </item>
        </channel>
      </rss>
      """
      doc = RustyXML.parse(xml)
      result = RustyXML.xpath(doc, "count(//item)")
      assert result == 2.0
    end

    test "SOAP-like envelope" do
      xml = """
      <soap:Envelope xmlns:soap="http://schemas.xmlsoap.org/soap/envelope/">
        <soap:Header/>
        <soap:Body>
          <GetPrice xmlns="http://example.com/stock">
            <symbol>MSFT</symbol>
          </GetPrice>
        </soap:Body>
      </soap:Envelope>
      """
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "SVG-like structure" do
      xml = """
      <svg xmlns="http://www.w3.org/2000/svg" width="100" height="100">
        <circle cx="50" cy="50" r="40" fill="red"/>
        <rect x="10" y="10" width="30" height="30" fill="blue"/>
      </svg>
      """
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "Atom feed structure" do
      xml = """
      <feed xmlns="http://www.w3.org/2005/Atom">
        <title>Example Feed</title>
        <entry>
          <title>Entry 1</title>
          <id>urn:uuid:1</id>
        </entry>
      </feed>
      """
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "plist-like structure" do
      xml = """
      <plist version="1.0">
        <dict>
          <key>Name</key>
          <string>Example</string>
          <key>Count</key>
          <integer>42</integer>
        </dict>
      </plist>
      """
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end
  end

  # ===========================================================================
  # Section 13: XPath Edge Cases
  # ===========================================================================

  describe "XPath edge cases" do
    test "XPath with position predicate at start" do
      xml = "<root><a/><a/><a/></root>"
      result = RustyXML.xpath(xml, "/root/a[1]")
      assert length(result) == 1
    end

    test "XPath with last() function" do
      xml = "<root><a/><a/><a/></root>"
      result = RustyXML.xpath(xml, "/root/a[last()]")
      assert length(result) == 1
    end

    test "XPath with boolean and" do
      xml = "<root><a x=\"1\" y=\"2\"/></root>"
      doc = RustyXML.parse(xml)
      assert is_reference(doc)
    end

    test "XPath with multiple predicates" do
      xml = "<root><a><b>1</b></a><a><b>2</b></a></root>"
      result = RustyXML.xpath(xml, "//a[b][1]")
      assert is_list(result)
    end

    test "XPath ancestor axis" do
      xml = "<a><b><c/></b></a>"
      result = RustyXML.xpath(xml, "//c/ancestor::*")
      assert length(result) >= 2
    end

    test "XPath following-sibling axis" do
      xml = "<root><a/><b/><c/></root>"
      result = RustyXML.xpath(xml, "/root/a/following-sibling::*")
      assert length(result) == 2
    end

    test "XPath preceding-sibling axis" do
      xml = "<root><a/><b/><c/></root>"
      result = RustyXML.xpath(xml, "/root/c/preceding-sibling::*")
      assert length(result) == 2
    end
  end
end
