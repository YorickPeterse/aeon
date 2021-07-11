# frozen_string_literal: true

module Inkoc
  module AST
    class For
      include Predicates
      include Inspect
      include TypeOperations

      attr_reader :binding, :error, :iterable, :body, :else_argument,
                  :else_body, :location

      def initialize(binding, error, iterable, body, else_argument, else_body, location)
        @binding = binding
        @error = error
        @iterable = iterable
        @body = body
        @else_argument = else_argument
        @else_body = else_body
        @location = location
      end

      def visitor_method
        :on_for
      end
    end
  end
end
