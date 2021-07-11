# frozen_string_literal: true

module Inkoc
  module AST
    class DestructArray
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :variables, :value, :location
      attr_accessor :value_kind

      def initialize(variables, value, location)
        @variables = variables
        @value = value
        @location = location
        @value_kind = :array
      end

      def visitor_method
        :on_destructure_array
      end

      def byte_array?
        @value_kind == :byte_array
      end

      def pair?
        @value_kind == :pair
      end

      def triple?
        @value_kind == :triple
      end
    end
  end
end
