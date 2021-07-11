# frozen_string_literal: true

module Inkoc
  module AST
    class If
      include Predicates
      include Inspect
      include TypeOperations

      attr_reader :conditions, :else_body, :location

      def initialize(conditions, else_body, location)
        @conditions = conditions
        @else_body = else_body
        @location = location
      end

      def visitor_method
        :on_if
      end
    end
  end
end
