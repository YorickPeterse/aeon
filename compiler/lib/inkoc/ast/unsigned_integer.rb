# frozen_string_literal: true

module Inkoc
  module AST
    class UnsignedInteger
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :value, :location

      def initialize(value, location)
        @value = value
        @location = location
      end

      def visitor_method
        :on_unsigned_integer
      end
    end
  end
end
