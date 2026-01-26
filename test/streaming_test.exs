defmodule RustyXML.StreamingTest do
  use ExUnit.Case, async: true

  describe "Native streaming functions" do
    test "streaming_new creates a parser reference" do
      parser = RustyXML.Native.streaming_new()
      assert is_reference(parser)
    end

    test "streaming_new_with_filter creates a parser reference" do
      parser = RustyXML.Native.streaming_new_with_filter("item")
      assert is_reference(parser)
    end

    test "streaming_feed returns available and buffer size" do
      parser = RustyXML.Native.streaming_new()
      {available, buffer_size} = RustyXML.Native.streaming_feed(parser, "<root>")
      assert is_integer(available)
      assert is_integer(buffer_size)
    end

    test "streaming_status returns parser status" do
      parser = RustyXML.Native.streaming_new()
      {available, buffer_size, has_pending} = RustyXML.Native.streaming_status(parser)
      assert is_integer(available)
      assert is_integer(buffer_size)
      assert is_boolean(has_pending)
    end

    test "streaming_take_events returns events list" do
      parser = RustyXML.Native.streaming_new()
      RustyXML.Native.streaming_feed(parser, "<root><item/></root>")
      events = RustyXML.Native.streaming_take_events(parser, 100)
      assert is_list(events)
      assert events != []
    end

    test "streaming_finalize returns remaining events" do
      parser = RustyXML.Native.streaming_new()
      RustyXML.Native.streaming_feed(parser, "<root><item/>")
      events = RustyXML.Native.streaming_finalize(parser)
      assert is_list(events)
    end

    test "can parse chunked input" do
      parser = RustyXML.Native.streaming_new()

      # Feed in chunks
      RustyXML.Native.streaming_feed(parser, "<root>")
      RustyXML.Native.streaming_feed(parser, "<item>1</item>")
      RustyXML.Native.streaming_feed(parser, "<item>2</item>")
      RustyXML.Native.streaming_feed(parser, "</root>")

      events = RustyXML.Native.streaming_take_events(parser, 100)
      final = RustyXML.Native.streaming_finalize(parser)

      all_events = events ++ final
      assert length(all_events) >= 5
    end

    test "filter only emits matching tags" do
      parser = RustyXML.Native.streaming_new_with_filter("item")

      RustyXML.Native.streaming_feed(parser, "<root><item/><other/><item/></root>")
      events = RustyXML.Native.streaming_take_events(parser, 100)
      final = RustyXML.Native.streaming_finalize(parser)

      all_events = events ++ final

      # Should only have item events (empty_element)
      item_events =
        Enum.filter(all_events, fn
          {:empty_element, name, _} -> name == "item"
          {:start_element, name, _} -> name == "item"
          {:end_element, name} -> name == "item"
          _ -> false
        end)

      assert length(item_events) >= 2
    end
  end

  describe "stream_tags/3 high-level API" do
    setup do
      # Create a temporary XML file for streaming tests
      path = Path.join(System.tmp_dir!(), "test_stream_#{:rand.uniform(1_000_000)}.xml")

      content = """
      <?xml version="1.0"?>
      <catalog>
        <item id="1"><name>First</name><price>10.00</price></item>
        <item id="2"><name>Second</name><price>20.00</price></item>
        <item id="3"><name>Third</name><price>30.00</price></item>
      </catalog>
      """

      File.write!(path, content)

      on_exit(fn -> File.rm(path) end)

      {:ok, path: path}
    end

    test "streams tags from file path", %{path: path} do
      events =
        path
        |> RustyXML.stream_tags(:item)
        |> Enum.to_list()

      # Should have events for all 3 items
      assert length(events) >= 3
    end

    test "streams tags from enumerable" do
      xml = "<root><item>1</item><item>2</item></root>"
      chunks = [xml]

      events =
        chunks
        |> RustyXML.stream_tags(:item)
        |> Enum.to_list()

      assert is_list(events)
    end
  end
end
