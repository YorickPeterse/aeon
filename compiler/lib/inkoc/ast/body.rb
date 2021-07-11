# frozen_string_literal: true

module Inkoc
  module AST
    class Body
      include TypeOperations
      include Predicates
      include Inspect

      attr_reader :location, :drop_variables
      attr_accessor :expressions, :locals

      # expr - The expressions of this body.
      # location - The SourceLocation of this node.
      def initialize(expr, location)
        @expressions = expr
        @location = location
        @locals = nil
        @drop_variables = []
      end

      def visitor_method
        :on_body
      end

      def multiple_expressions?
        @expressions.length >= 1
      end

      def last_expression
        @expressions.last
      end

      def prepend(nodes)
        return if nodes.empty?

        @expressions = nodes + @expressions
      end

      def empty?
        @expressions.empty?
      end

      def location_of_last_expression
        last_expression&.location || location
      end

      def returns?
        @expressions.any? { |n| n.is_a?(AST::Return) }
      end

      def throws?
        @expressions.any? { |n| n.is_a?(AST::Throw) }
      end
    end
  end
end
