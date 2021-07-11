# frozen_string_literal: true

module Inkoc
  module AST
    class DestructArrayVariable
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :variable, :value_type, :location
      attr_accessor :symbol

      def initialize(variable, value_type, mutable, location)
        @variable = variable
        @value_type = value_type
        @mutable = mutable
        @location = location
        @symbol = symbol
      end

      def mutable?
        @mutable
      end
    end
  end
end
