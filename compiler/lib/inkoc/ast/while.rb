# frozen_string_literal: true

module Inkoc
  module AST
    class While
      include Predicates
      include Inspect
      include TypeOperations

      attr_reader :condition, :body, :location

      def initialize(condition, body, location)
        @condition = condition
        @body = body
        @location = location
      end

      def visitor_method
        :on_while
      end
    end
  end
end
