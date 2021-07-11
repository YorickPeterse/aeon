# frozen_string_literal: true

module Inkoc
  module Codegen
    class Serializer
      SIGNATURE = 'inko'.bytes
      VERSION = 1

      INTEGER_LITERAL = 0
      UNSIGNED_INTEGER_LITERAL = 1
      FLOAT_LITERAL = 2
      STRING_LITERAL = 3

      U8_RANGE = 0..255
      U16_RANGE = 0..65535
      U32_RANGE = 0..4294967295
      U64_RANGE = 0..18446744073709551615

      I32_RANGE = -2147483648..2147483647
      I64_RANGE = -9223372036854775808..9223372036854775807

      # The range of values that can be encoded as an signed 64 bits integer.
      #
      # These values are based on Rust's `std::i64::MIN` and `std::i64::MAX`.
      INTEGER_RANGE = -9_223_372_036_854_775_808..9_223_372_036_854_775_807

      def initialize(compiler, mod)
        @compiler = compiler
        @module = mod
      end

      def serialize_to_file(path)
        File.open(path, 'wb') do |handle|
          output = []

          serialize(output)
          handle.write(output.pack('C*'))
        end

        nil
      end

      def serialize(output)
        mods = @compiler.modules

        SIGNATURE.each do |byte|
          u8(byte, output)
        end

        u8(VERSION, output)
        entry_point(output)
        u64(mods.length, output)

        mods.each do |mod|
          code_module(mod, output)
        end

        output
      end

      def entry_point(output)
        string(@module.name.to_s, output)
      end

      def code_module(mod, output)
        start = output.length

        # This is the placeholder for the body size, which we'll update below.
        u64(0, output)

        size_before = output.length

        array(mod.literals.to_a, :literal, output, length: :u32)
        compiled_code(mod.body, output)

        size = output.length - size_before

        # Having to duplicate u64() here is unfortunate, but it's the most
        # memory/CPU efficient way to update the size placeholder.
        output[start] = (size & 0xFF)
        output[start + 1] = ((size >> 8) & 0xFF)
        output[start + 2] = ((size >> 16) & 0xFF)
        output[start + 3] = ((size >> 24) & 0xFF)

        output[start + 4] = ((size >> 32) & 0xFF)
        output[start + 5] = ((size >> 40) & 0xFF)
        output[start + 6] = ((size >> 48) & 0xFF)
        output[start + 7] = ((size >> 56) & 0xFF)
      end

      def string(str, output)
        str = str.to_s

        u64(str.bytesize, output)
        str.each_byte { |byte| u8(byte, output) }
      end

      def u8(num, output)
        validate_range!(num, U8_RANGE)

        output << num
      end

      def u16(num, output)
        validate_range!(num, U16_RANGE)

        output << (num & 0xFF)
        output << ((num >> 8) & 0xFF)
      end

      def u32(num, output)
        validate_range!(num, U32_RANGE)

        output << (num & 0xFF)
        output << ((num >> 8) & 0xFF)
        output << ((num >> 16) & 0xFF)
        output << ((num >> 24) & 0xFF)
      end

      def u64(num, output)
        validate_range!(num, U64_RANGE)

        output << (num & 0xFF)
        output << ((num >> 8) & 0xFF)
        output << ((num >> 16) & 0xFF)
        output << ((num >> 24) & 0xFF)

        output << ((num >> 32) & 0xFF)
        output << ((num >> 40) & 0xFF)
        output << ((num >> 48) & 0xFF)
        output << ((num >> 56) & 0xFF)
      end

      def i32(num, output)
        validate_range!(num, I32_RANGE)

        output << (num & 0xFF)
        output << ((num >> 8) & 0xFF)
        output << ((num >> 16) & 0xFF)
        output << ((num >> 24) & 0xFF)
      end

      def i64(num, output)
        validate_range!(num, I64_RANGE)

        output << (num & 0xFF)
        output << ((num >> 8) & 0xFF)
        output << ((num >> 16) & 0xFF)
        output << ((num >> 24) & 0xFF)

        output << ((num >> 32) & 0xFF)
        output << ((num >> 40) & 0xFF)
        output << ((num >> 48) & 0xFF)
        output << ((num >> 56) & 0xFF)
      end

      def f64(num, output)
        # Ruby doesn't offer a better way for packing a Float :<
        [num].pack('E').each_byte do |byte|
          output << byte
        end
      end

      def boolean(val, output)
        u8(val ? 1 : 0, output)
      end

      def array(values, encoder, output, length:)
        public_send(length, values.length, output)

        values.each { |value| send(encoder, value, output) }
      end

      def instruction(ins, output)
        u8(ins.index, output)
        u8(ins.arguments.length, output)
        ins.arguments.each { |arg| u16(arg, output) }
        u16(ins.line, output)
      end

      def catch_entry(entry, output)
        u16(entry.start, output)
        u16(entry.stop, output)
        u16(entry.jump_to, output)
      end

      def integer_literal(value, output)
        u8(INTEGER_LITERAL, output)
        i64(value, output)
      end

      def integer_literal(value, output)
        u8(UNSIGNED_INTEGER_LITERAL, output)
        u64(value, output)
      end

      def float_literal(value, output)
        u8(FLOAT_LITERAL, output)
        f64(value, output)
      end

      def string_literal(value, output)
        u8(STRING_LITERAL, output)
        string(value, output)
      end

      def literal(value, output)
        case value
        when Integer
          integer_literal(value, output)
        when UnsignedInteger
          unsigned_integer_literal(value, output)
        when Float
          float_literal(value, output)
        when String, Symbol
          string_literal(value, output)
        else
          raise TypeError, "Unsupported literal type: #{value.inspect}"
        end
      end

      # rubocop: disable Metrics/AbcSize
      def compiled_code(code, output)
        u32(code.name, output)
        u32(code.file, output)
        u16(code.line, output)
        array(code.arguments, :u32, output, length: :u8)
        u16(code.locals, output)
        u16(code.registers, output)
        boolean(code.captures, output)
        array(code.instructions, :instruction, output, length: :u32)
        array(code.code_objects.to_a, :compiled_code, output, length: :u16)
        array(code.catch_table.to_a, :catch_entry, output, length: :u16)
      end

      def validate_range!(value, range)
        return if range.cover?(value)

        raise(
          ArgumentError,
          "The value #{value.inspect} is not in the range #{range.inspect}"
        )
      end
    end
  end
end
